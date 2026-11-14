//! AT command transport over the modem's control channel (/dev/radio/pttycmd1, a pseudo-terminal
//! created by gsm0710muxd; one opener at a time). A single bounded worker thread owns the channel:
//! the core loop hands it jobs (a short list of commands) and picks up the results without ever
//! waiting. The channel is opened per job and closed afterwards, so `walpad-modem data` can use it
//! between jobs. Every command has a hard deadline (5 s on the tablet).
use std::os::fd::OwnedFd;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;
mod worker;

pub use worker::{Done, Job, Outcome, Recv, Worker};

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
