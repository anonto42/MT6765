//! AT command transport over the modem's control channel (/dev/radio/pttycmd1, a pseudo-terminal
//! created by gsm0710muxd; one opener at a time). A single bounded worker thread owns the channel:
//! the core loop hands it jobs (a short list of commands) and picks up the results without ever
//! waiting. The channel is opened per job and closed afterwards, so `walpad-modem data` can use it
//! between jobs. Every command has a hard deadline (5 s on the tablet).
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Final {
    Ok,
    Error,
    /// `+CME ERROR: <code or text>`.
    Cme(String),
    /// `+CMS ERROR: <code>`.
    Cms(String),
    NoCarrier,
    /// Voice call results (`ATD`): the line was busy, nobody answered, no dial tone.
    Busy,
    NoAnswer,
    NoDialtone,
}

/// The information lines of one command (echo, blank lines and the final result removed; URCs are
/// kept: the parsers pick the lines they know).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub lines: Vec<String>,
    pub fin: Final,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtError {
    /// The channel could not be opened (no muxd, no modem).
    Open(String),
    /// No final result before the deadline.
    Timeout,
    Io(String),
}

/// One command. A secret command (the SIM PIN) prints as `AT+CPIN=***` and its text never leaves
/// the channel.
#[derive(Clone, PartialEq, Eq)]
pub struct AtCmd {
    text: String,
    secret: bool,
    /// Sent after the `> ` prompt (an SMS PDU for `AT+CMGS`), ended by Ctrl-Z. Always private.
    payload: Option<String>,
    /// Overrides the per-command deadline (an SMS submit takes longer than a status query).
    timeout: Option<Duration>,
}

impl AtCmd {
    pub fn new(text: impl Into<String>) -> AtCmd {
        AtCmd {
            text: text.into(),
            secret: false,
            payload: None,
            timeout: None,
        }
    }
    pub fn secret(text: String) -> AtCmd {
        AtCmd {
            text,
            secret: true,
            payload: None,
            timeout: None,
        }
    }
    /// The same command with its own deadline (a long listing).
    pub fn timeout(mut self, d: Duration) -> AtCmd {
        self.timeout = Some(d);
        self
    }
    /// A command with a prompted payload (`AT+CMGS=<len>` + PDU): prints as `AT+CMGS=***`.
    pub fn with_payload(text: String, payload: String, timeout: Duration) -> AtCmd {
        AtCmd {
            text,
            secret: true,
            payload: Some(payload),
            timeout: Some(timeout),
        }
    }
    /// What may be logged.
    pub fn display(&self) -> String {
        if self.secret {
            match self.text.split_once('=') {
                Some((head, _)) => format!("{head}=***"),
                // `ATD<number>;`: the verb only
                None => format!("{}***", self.text.chars().take(3).collect::<String>()),
            }
        } else {
            self.text.clone()
        }
    }
}

impl std::fmt::Debug for AtCmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display())
    }
}

fn final_of(line: &str) -> Option<Final> {
    Some(match line {
        "OK" => Final::Ok,
        "ERROR" => Final::Error,
        "NO CARRIER" => Final::NoCarrier,
        "BUSY" => Final::Busy,
        "NO ANSWER" => Final::NoAnswer,
        "NO DIALTONE" => Final::NoDialtone,
        l => {
            if let Some(e) = l.strip_prefix("+CME ERROR:") {
                Final::Cme(e.trim().to_string())
            } else {
                Final::Cms(l.strip_prefix("+CMS ERROR:")?.trim().to_string())
            }
        }
    })
}

/// Split what the channel said after `cmd` was sent into a `Reply`; None while the final result
/// has not arrived. Lines end in CR LF (a stray CR from the echo is tolerated).
pub fn split_reply(cmd: &str, text: &str) -> Option<Reply> {
    let mut lines = Vec::new();
    for raw in text.split(['\n', '\r']) {
        let l = raw.trim();
        if l.is_empty() || l.eq_ignore_ascii_case(cmd.trim()) {
            continue;
        }
        if let Some(fin) = final_of(l) {
            return Some(Reply { lines, fin });
        }
        lines.push(l.to_string());
    }
    None
}

/// An open AT channel (non-blocking, raw mode).
pub struct Port {
    fd: OwnedFd,
}

impl Port {
    pub fn open(path: &Path) -> Result<Port, AtError> {
        use rustix::fs::{Mode, OFlags};
        let fd = rustix::fs::open(
            path,
            OFlags::RDWR | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| AtError::Open(e.to_string()))?;
        // raw: no echo back to the multiplexer, no line editing, no CR/LF translation
        if let Ok(mut t) = rustix::termios::tcgetattr(&fd) {
            t.make_raw();
            let _ = rustix::termios::tcsetattr(&fd, rustix::termios::OptionalActions::Now, &t);
        }
        Ok(Port { fd })
    }

    fn wait(&self, flags: rustix::event::PollFlags, deadline: Instant) -> Result<bool, AtError> {
        use rustix::event::{poll, PollFd, Timespec};
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(false);
        }
        let ms = left.as_millis().clamp(1, 1000) as i64;
        let t = Timespec {
            tv_sec: ms / 1000,
            tv_nsec: (ms % 1000) * 1_000_000,
        };
        let mut fds = [PollFd::new(&self.fd, flags)];
        match poll(&mut fds, Some(&t)) {
            Ok(n) => Ok(n > 0),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(e) => Err(AtError::Io(e.to_string())),
        }
    }

    /// Throw away whatever is waiting (URCs between commands).
    fn drain(&self) {
        let mut buf = [0u8; 512];
        while let Ok(n) = rustix::io::read(&self.fd, &mut buf) {
            if n == 0 {
                break;
            }
        }
    }

    fn write_all(&self, mut b: &[u8], deadline: Instant) -> Result<(), AtError> {
        while !b.is_empty() {
            match rustix::io::write(&self.fd, b) {
                Ok(n) => b = &b[n..],
                Err(rustix::io::Errno::AGAIN) => {
                    if Instant::now() >= deadline {
                        return Err(AtError::Timeout);
                    }
                    self.wait(rustix::event::PollFlags::OUT, deadline)?;
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(AtError::Io(e.to_string())),
            }
        }
        Ok(())
    }

    /// Send one command and collect its reply within `timeout`.
    pub fn command(&mut self, cmd: &AtCmd, timeout: Duration) -> Result<Reply, AtError> {
        let deadline = Instant::now() + cmd.timeout.unwrap_or(timeout);
        self.drain();
        let mut line = cmd.text.clone().into_bytes();
        line.push(b'\r');
        self.write_all(&line, deadline)?;
        let mut got: Vec<u8> = Vec::new();
        let mut buf = [0u8; 1024];
        if let Some(payload) = &cmd.payload {
            // wait for the `> ` prompt (or an early error), then send the PDU and Ctrl-Z
            loop {
                if let Some(r) = split_reply(&cmd.text, &String::from_utf8_lossy(&got)) {
                    return Ok(r); // refused before the prompt
                }
                if got.contains(&b'>') {
                    break;
                }
                match rustix::io::read(&self.fd, &mut buf) {
                    Ok(0) => return Err(AtError::Io("channel closed".into())),
                    Ok(n) => {
                        got.extend_from_slice(&buf[..n]);
                        continue;
                    }
                    Err(rustix::io::Errno::AGAIN) | Err(rustix::io::Errno::INTR) => {}
                    Err(e) => return Err(AtError::Io(e.to_string())),
                }
                if Instant::now() >= deadline {
                    let _ = self.write_all(&[0x1B], Instant::now() + Duration::from_millis(500));
                    return Err(AtError::Timeout);
                }
                self.wait(rustix::event::PollFlags::IN, deadline)?;
            }
            got.clear();
            let mut body = payload.clone().into_bytes();
            body.push(0x1A);
            self.write_all(&body, deadline)?;
        }
        loop {
            match rustix::io::read(&self.fd, &mut buf) {
                Ok(0) => return Err(AtError::Io("channel closed".into())),
                Ok(n) => {
                    got.extend_from_slice(&buf[..n]);
                    if got.len() > 64 * 1024 {
                        return Err(AtError::Io("reply too long".into()));
                    }
                    if let Some(r) = split_reply(&cmd.text, &String::from_utf8_lossy(&got)) {
                        return Ok(r);
                    }
                    continue;
                }
                Err(rustix::io::Errno::AGAIN) | Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(AtError::Io(e.to_string())),
            }
            if Instant::now() >= deadline {
                return Err(AtError::Timeout);
            }
            self.wait(rustix::event::PollFlags::IN, deadline)?;
        }
    }
}

pub type Outcome = Result<Reply, AtError>;

pub struct Job<K> {
    pub kind: K,
    pub cmds: Vec<AtCmd>,
}

/// The results in command order; a job stops at the first transport error (open, timeout, I/O),
/// so `results` can be shorter than the job.
pub struct Done<K> {
    pub kind: K,
    pub results: Vec<Outcome>,
}

pub enum Recv<K> {
    Done(Done<K>),
    Empty,
    /// The worker thread is gone.
    Gone,
}

/// The single AT worker thread. Dropping it ends the thread after its current job.
pub struct Worker<K> {
    tx: Sender<Job<K>>,
    rx: Receiver<Done<K>>,
}

impl<K: Send + 'static> Worker<K> {
    pub fn spawn(path: PathBuf, per_cmd: Duration) -> std::io::Result<Worker<K>> {
        let (tx, jobs) = channel::<Job<K>>();
        let (done, rx) = channel::<Done<K>>();
        std::thread::Builder::new()
            .name("walpad-at".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let results = run_job(&path, &job.cmds, per_cmd);
                    if done
                        .send(Done {
                            kind: job.kind,
                            results,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Worker { tx, rx })
    }

    pub fn send(&self, job: Job<K>) -> bool {
        self.tx.send(job).is_ok()
    }

    /// A finished job, if any (never blocks).
    pub fn try_recv(&self) -> Recv<K> {
        match self.rx.try_recv() {
            Ok(d) => Recv::Done(d),
            Err(TryRecvError::Empty) => Recv::Empty,
            Err(TryRecvError::Disconnected) => Recv::Gone,
        }
    }
}

fn run_job(path: &Path, cmds: &[AtCmd], per_cmd: Duration) -> Vec<Outcome> {
    let mut port = match Port::open(path) {
        Ok(p) => p,
        Err(e) => return vec![Err(e)],
    };
    let mut out = Vec::new();
    for c in cmds {
        let r = port.command(c, per_cmd);
        let stop = r.is_err();
        out.push(r);
        if stop {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_split_from_transcripts() {
        let r = split_reply("AT+CSQ", "AT+CSQ\r\r\n+CSQ: 29, 5\r\n\r\nOK\r\n").unwrap();
        assert_eq!(r.lines, ["+CSQ: 29, 5"]);
        assert_eq!(r.fin, Final::Ok);
        assert_eq!(
            split_reply("AT+CSQ", "\r\n+CSQ: 29, 5\r\n"),
            None,
            "no final yet"
        );
        let e = split_reply("AT+CPIN?", "\r\n+CME ERROR: 10\r\n").unwrap();
        assert_eq!(e.fin, Final::Cme("10".into()));
        assert_eq!(
            split_reply("AT+CMGS", "+CMS ERROR: 500\r\n").unwrap().fin,
            Final::Cms("500".into())
        );
        assert_eq!(
            split_reply("ATD", "NO CARRIER\r\n").unwrap().fin,
            Final::NoCarrier
        );
        // URCs stay in the lines (the parsers ignore what they do not know)
        let u = split_reply("AT+CFUN?", "+EIND: 64\r\n+CFUN: 1\r\nOK\r\n").unwrap();
        assert_eq!(u.lines, ["+EIND: 64", "+CFUN: 1"]);
    }

    #[test]
    fn a_secret_command_never_prints() {
        let c = AtCmd::secret("AT+CPIN=\"4711\"".into());
        assert_eq!(format!("{c:?}"), "AT+CPIN=***");
        assert!(!c.display().contains("4711"));
        assert_eq!(AtCmd::new("AT+CSQ").display(), "AT+CSQ");
    }

    #[test]
    fn opening_a_missing_channel_fails_fast() {
        let r = run_job(
            Path::new("/nonexistent/radio/pttycmd1"),
            &[AtCmd::new("AT")],
            Duration::from_millis(100),
        );
        assert!(matches!(r.as_slice(), [Err(AtError::Open(_))]));
    }
}
