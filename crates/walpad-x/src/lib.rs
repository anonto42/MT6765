//! X connection helper shared by walpad-core and walpad-shell: a unix stream whose every wait has
//! a deadline, so a hung X server can never hang a WalPad process (plan rule 9).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::{PollMode, RustConnection, Stream};
use x11rb::utils::RawFdContainer;

pub const X_TIMEOUT_S: i64 = 2;

/// Bridge error (message only). Deliberately not `Display`, so the blanket `From` below is coherent.
#[derive(Debug)]
pub struct XError(pub String);
impl<E: std::fmt::Display> From<E> for XError {
    fn from(e: E) -> Self {
        XError(e.to_string())
    }
}

/// Unix stream with a deadline on every poll.
pub struct TimedStream(UnixStream);

impl Stream for TimedStream {
    fn poll(&self, mode: PollMode) -> std::io::Result<()> {
        let mut flags = PollFlags::empty();
        if mode.readable() {
            flags |= PollFlags::IN;
        }
        if mode.writable() {
            flags |= PollFlags::OUT;
        }
        loop {
            let mut fds = [PollFd::new(&self.0, flags)];
            let t = Timespec {
                tv_sec: X_TIMEOUT_S,
                tv_nsec: 0,
            };
            match poll(&mut fds, Some(&t)) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "X server did not respond",
                    ))
                }
                Ok(_) => return Ok(()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    fn read(&self, buf: &mut [u8], _fds: &mut Vec<RawFdContainer>) -> std::io::Result<usize> {
        (&self.0).read(buf)
    }
    fn write(&self, buf: &[u8], _fds: &mut Vec<RawFdContainer>) -> std::io::Result<usize> {
        (&self.0).write(buf)
    }
}

pub struct XConn {
    pub conn: RustConnection<TimedStream>,
    /// A duplicate of the socket, for `poll()` in an event loop. Always drain
    /// `conn.poll_for_event()` before waiting on it (the connection may have buffered events).
    pub wait_fd: UnixStream,
    pub root: Window,
}

/// Connect to the X server listening on `socket` (e.g. /tmp/.X11-unix/X0).
pub fn connect(socket: &Path) -> Result<XConn, XError> {
    let s = UnixStream::connect(socket)?;
    s.set_nonblocking(true)?;
    let wait_fd = s.try_clone()?;
    let conn = RustConnection::connect_to_stream(TimedStream(s), 0)?;
    let root = conn
        .setup()
        .roots
        .first()
        .ok_or_else(|| XError("no screen".into()))?
        .root;
    Ok(XConn {
        conn,
        wait_fd,
        root,
    })
}

/// Wait until the X socket is readable or `timeout_ms` elapses (None = a long default).
pub fn wait_readable(fd: &UnixStream, timeout_ms: Option<u64>) -> bool {
    let ms = timeout_ms.unwrap_or(60_000).min(i32::MAX as u64) as i64;
    let t = Timespec {
        tv_sec: ms / 1000,
        tv_nsec: (ms % 1000) * 1_000_000,
    };
    loop {
        let mut fds = [PollFd::new(fd, PollFlags::IN)];
        match poll(&mut fds, Some(&t)) {
            Ok(n) => return n > 0,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return false,
        }
    }
}

/// Wait until any of `fds` is readable (or the timeout). Returns one flag per fd.
pub fn wait_any(fds: &[&UnixStream], timeout_ms: Option<u64>) -> Vec<bool> {
    let ms = timeout_ms.unwrap_or(60_000).min(i32::MAX as u64) as i64;
    let t = Timespec {
        tv_sec: ms / 1000,
        tv_nsec: (ms % 1000) * 1_000_000,
    };
    loop {
        let mut pfds: Vec<PollFd<'_>> = fds
            .iter()
            .map(|f| PollFd::new(*f, PollFlags::IN | PollFlags::HUP))
            .collect();
        match poll(&mut pfds, Some(&t)) {
            Ok(_) => {
                return pfds
                    .iter()
                    .map(|p| {
                        p.revents()
                            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
                    })
                    .collect()
            }
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return vec![false; fds.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn connecting_to_a_missing_server_fails_fast() {
        let t = Instant::now();
        assert!(connect(Path::new("/tmp/.X11-unix/X998")).is_err());
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn wait_any_reports_which_socket_is_ready() {
        use std::io::Write;
        let (a, mut a2) = UnixStream::pair().unwrap();
        let (b, _b2) = UnixStream::pair().unwrap();
        assert_eq!(wait_any(&[&a, &b], Some(20)), vec![false, false]);
        a2.write_all(b"x").unwrap();
        assert_eq!(wait_any(&[&a, &b], Some(200)), vec![true, false]);
    }

    #[test]
    fn wait_times_out_on_a_quiet_socket() {
        let (a, _b) = UnixStream::pair().unwrap();
        let t = Instant::now();
        assert!(!wait_readable(&a, Some(50)));
        assert!(t.elapsed() >= Duration::from_millis(40));
    }
}
