//! AF_UNIX datagram sockets in the abstract namespace with MediaTek's fixed address length, and the
//! pseudo-terminal the NMEA sentences are copied to.
use crate::wire::ADDR_LEN;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

/// `sockaddr_un` for abstract `name`, padded with NULs to the full 110-byte length that mnld and
/// the HAL use (abstract addresses compare the whole length, so a shorter one never matches).
pub fn abstract_addr(name: &str) -> (libc::sockaddr_un, libc::socklen_t) {
    // SAFETY: sockaddr_un is plain old data; all-zero is a valid value.
    let mut a: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    a.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (i, b) in name.bytes().take(ADDR_LEN - 4).enumerate() {
        a.sun_path[1 + i] = b as libc::c_char;
    }
    (a, ADDR_LEN as libc::socklen_t)
}

pub struct Sock {
    fd: OwnedFd,
}

impl Sock {
    /// Bind abstract `name` (we take the HAL's place: nothing else may own it).
    pub fn bind(name: &str) -> io::Result<Sock> {
        // SAFETY: plain socket(2); the result is checked and wrapped at once.
        let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `raw` is a fresh descriptor that nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let big: libc::c_int = 1 << 20;
        // SAFETY: valid pointer and length for an int option; failure is harmless (default size).
        unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                (&big as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
        let (a, len) = abstract_addr(name);
        // SAFETY: `a` is a valid sockaddr_un of `len` bytes.
        let r = unsafe { libc::bind(fd.as_raw_fd(), (&a as *const libc::sockaddr_un).cast(), len) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Sock { fd })
    }

    /// Send one datagram to abstract `name` (ECONNREFUSED = nobody bound there yet).
    pub fn send_to(&self, name: &str, data: &[u8]) -> io::Result<()> {
        let (a, len) = abstract_addr(name);
        // SAFETY: `data` and `a` are valid for the given lengths.
        let r = unsafe {
            libc::sendto(
                self.fd.as_raw_fd(),
                data.as_ptr().cast(),
                data.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
                (&a as *const libc::sockaddr_un).cast(),
                len,
            )
        };
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Wait up to `timeout_ms` for a datagram. `Ok(None)` = timeout (or an interrupted wait).
    pub fn recv(&self, buf: &mut [u8], timeout_ms: i32) -> io::Result<Option<usize>> {
        let mut p = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        let n = unsafe { libc::poll(&mut p, 1, timeout_ms) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(e)
            };
        }
        if n == 0 {
            return Ok(None);
        }
        // SAFETY: `buf` is valid for `buf.len()` bytes.
        let r = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                libc::MSG_DONTWAIT,
            )
        };
        if r < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::WouldBlock {
                Ok(None)
            } else {
                Err(e)
            };
        }
        Ok(Some(r as usize))
    }
}

/// A pty master that receives every NMEA sentence; `link` points at its slave so gpsd, `cat` or an
/// app can open a normal tty path.  Writes never block: with nobody reading, the tty buffer fills
/// and further lines are dropped.
pub struct NmeaOut {
    master: OwnedFd,
    /// Held open so the master never sees a hangup when a reader goes away.
    _slave: OwnedFd,
    link: PathBuf,
}

impl NmeaOut {
    pub fn open(link: &Path) -> io::Result<NmeaOut> {
        use rustix::fs::{fcntl_setfl, Mode, OFlags};
        use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
        use rustix::termios::{tcgetattr, tcsetattr, OptionalActions};
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let name = ptsname(&master, Vec::new())?;
        let slave = rustix::fs::open(
            name.to_str().map_err(|_| io::Error::other("pty name"))?,
            OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        if let Ok(mut t) = tcgetattr(&slave) {
            t.make_raw();
            let _ = tcsetattr(&slave, OptionalActions::Now, &t);
        }
        fcntl_setfl(&master, OFlags::NONBLOCK)?;
        if let Some(d) = link.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut tmp = link.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let _ = std::fs::remove_file(&tmp);
        std::os::unix::fs::symlink(name.to_str().unwrap_or(""), &tmp)?;
        std::fs::rename(&tmp, link)?;
        Ok(NmeaOut {
            master: OwnedFd::from(master),
            _slave: OwnedFd::from(slave),
            link: link.to_path_buf(),
        })
    }

    /// One sentence plus CR LF; errors (full buffer) are ignored on purpose.
    pub fn write_line(&self, line: &str) {
        let mut b = Vec::with_capacity(line.len() + 2);
        b.extend_from_slice(line.as_bytes());
        b.extend_from_slice(b"\r\n");
        let _ = rustix::io::write(&self.master, &b);
    }
}

impl Drop for NmeaOut {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.link);
    }
}
