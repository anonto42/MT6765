//! wpa_supplicant control-interface client: unix datagram sockets bound at /run/walpad/wpa-<pid>
//! (requests) and /run/walpad/wpa-<pid>e (ATTACHed, unsolicited events). Every request has a 5 s
//! timeout; stale replies are drained first; any transport error drops the client so the manager
//! reconnects (e.g. when the supplicant restarts).
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum CtrlError {
    /// Socket missing / refused / I/O error: the supplicant is not there (yet).
    Down(String),
    Timeout,
    /// The supplicant answered FAIL (or an unexpected reply).
    Fail(String),
}

pub struct CtrlClient {
    req: UnixDatagram,
    ev: UnixDatagram,
    paths: [PathBuf; 2],
}

impl Drop for CtrlClient {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn bind_to(local: &Path, server: &Path) -> std::io::Result<UnixDatagram> {
    let _ = std::fs::remove_file(local);
    let s = UnixDatagram::bind(local)?;
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(local, std::fs::Permissions::from_mode(0o600));
    }
    s.connect(server)?;
    Ok(s)
}

impl CtrlClient {
    /// `run_dir` is where our client sockets live (/run/walpad); `server` the supplicant's socket.
    pub fn connect(run_dir: &Path, server: &Path) -> Result<Self, CtrlError> {
        let down = |e: std::io::Error| CtrlError::Down(e.to_string());
        let pid = std::process::id();
        let p1 = run_dir.join(format!("wpa-{pid}"));
        let p2 = run_dir.join(format!("wpa-{pid}e"));
        let req = bind_to(&p1, server).map_err(down)?;
        let ev = bind_to(&p2, server).map_err(down)?;
        req.set_read_timeout(Some(REQUEST_TIMEOUT)).map_err(down)?;
        req.set_write_timeout(Some(REQUEST_TIMEOUT)).map_err(down)?;
        ev.set_write_timeout(Some(REQUEST_TIMEOUT)).map_err(down)?;
        let c = CtrlClient {
            req,
            ev,
            paths: [p1, p2],
        };
        if c.request("PING")?.trim() != "PONG" {
            return Err(CtrlError::Fail("no PONG".into()));
        }
        // ATTACH on the event socket
        c.ev.set_read_timeout(Some(REQUEST_TIMEOUT)).map_err(down)?;
        c.ev.send(b"ATTACH").map_err(down)?;
        let mut buf = [0u8; 256];
        let n = c.ev.recv(&mut buf).map_err(|_| CtrlError::Timeout)?;
        if !buf[..n].starts_with(b"OK") {
            return Err(CtrlError::Fail("ATTACH refused".into()));
        }
        c.ev.set_nonblocking(true).map_err(down)?;
        Ok(c)
    }

    fn drain(&self) {
        let _ = self.req.set_nonblocking(true);
        let mut buf = [0u8; 4096];
        while self.req.recv(&mut buf).is_ok() {}
        let _ = self.req.set_nonblocking(false);
    }

    /// Send one command and return the reply text. `FAIL*` replies are `Err(Fail)`.
    pub fn request(&self, cmd: &str) -> Result<String, CtrlError> {
        self.drain();
        self.req
            .send(cmd.as_bytes())
            .map_err(|e| CtrlError::Down(e.to_string()))?;
        let mut buf = vec![0u8; 64 * 1024];
        let n = self.req.recv(&mut buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => CtrlError::Timeout,
            _ => CtrlError::Down(e.to_string()),
        })?;
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        if text.starts_with("FAIL") {
            // only the command verb goes into the error: arguments may carry a PSK
            Err(CtrlError::Fail(format!(
                "{} -> {}",
                cmd.split_whitespace().next().unwrap_or(""),
                text.trim()
            )))
        } else {
            Ok(text)
        }
    }

    /// Next pending unsolicited event line, if any (non-blocking).
    pub fn poll_event(&self) -> Result<Option<String>, CtrlError> {
        let mut buf = vec![0u8; 8192];
        match self.ev.recv(&mut buf) {
            Ok(n) => Ok(Some(String::from_utf8_lossy(&buf[..n]).into_owned())),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(CtrlError::Down(e.to_string())),
        }
    }
}
