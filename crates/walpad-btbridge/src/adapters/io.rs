use crate::h4::{self, Deframer, Next};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use rustix::io::Errno;
use std::io;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

/// Largest single read we issue (the stpbt driver caps a read at 2048, vhci frames at 1028).
pub const READ_BUF: usize = 4096;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub to_host_packets: u64,
    pub to_host_bytes: u64,
    pub to_controller_packets: u64,
    pub to_controller_bytes: u64,
    pub dropped_bytes: u64,
    pub rejected_host_packets: u64,
    pub hw_error_events: u64,
    pub patched_replies: u64,
    pub init_ok: u64,
    pub init_skipped: u64,
    pub init_failed: u64,
}

impl Stats {
    pub fn render(&self) -> String {
        format!(
            "to_host_packets={}\nto_host_bytes={}\nto_controller_packets={}\nto_controller_bytes={}\n\
             dropped_bytes={}\nrejected_host_packets={}\nhw_error_events={}\npatched_replies={}\n\
             init_ok={}\ninit_skipped={}\ninit_failed={}\n",
            self.to_host_packets,
            self.to_host_bytes,
            self.to_controller_packets,
            self.to_controller_bytes,
            self.dropped_bytes,
            self.rejected_host_packets,
            self.hw_error_events,
            self.patched_replies,
            self.init_ok,
            self.init_skipped,
            self.init_failed
        )
    }
}

pub(crate) fn timespec(d: Duration) -> Timespec {
    Timespec {
        tv_sec: d.as_secs() as i64,
        tv_nsec: d.subsec_nanos() as i64,
    }
}

/// Wait until `fd` is readable (or hung up); false on timeout.
pub fn wait_readable<F: AsFd>(fd: F, timeout: Duration) -> io::Result<bool> {
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    match poll(&mut fds, Some(&timespec(timeout))) {
        Ok(n) => Ok(n > 0),
        Err(Errno::INTR) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Read once from a non-blocking fd; `Ok(None)` when nothing is available.
pub fn read_some<F: AsFd>(fd: F, buf: &mut [u8]) -> io::Result<Option<usize>> {
    match rustix::io::read(fd, buf) {
        Ok(n) => Ok(Some(n)),
        Err(Errno::AGAIN) | Err(Errno::INTR) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Write all of `data`, retrying EAGAIN (the stpbt driver returns it when the STP queue is full,
/// and its poll() always says writable, so we back off by sleeping) and continuing partial writes.
pub fn write_all_timeout<F: AsFd>(fd: F, mut data: &[u8], timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while !data.is_empty() {
        match rustix::io::write(&fd, data) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "write returned 0")),
            Ok(n) => data = &data[n..],
            Err(Errno::AGAIN) | Err(Errno::INTR) => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "write timed out (queue full)",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Ask vhci for a primary (BR/EDR+LE) controller and return its hciN index.
///
/// vhci creates a primary device by itself 1 s after open if nobody asks; our request then fails
/// with EBADFD, which is fine: the creation notice is read either way.
pub fn create_hci<F: AsFd>(vhci: F, timeout: Duration) -> io::Result<u16> {
    match rustix::io::write(&vhci, &[h4::VENDOR, 0x00]) {
        Ok(_) | Err(Errno::BADFD) => {}
        Err(e) => return Err(e.into()),
    }
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; READ_BUF];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "no device-created notice from vhci",
            ));
        }
        if !wait_readable(&vhci, left)? {
            continue;
        }
        if let Some(n) = read_some(&vhci, &mut buf)? {
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "vhci closed"));
            }
            if let [h4::VENDOR, _opcode, lo, hi] = buf[..n] {
                return Ok(u16::from_le_bytes([lo, hi]));
            }
        }
    }
}

/// Send one HCI command to the controller and wait for its Command Complete; returns the status.
/// Other packets read meanwhile are dropped (nothing is attached yet), counted in the result.
pub fn controller_command<F: AsFd>(
    ctrl: F,
    cmd: &[u8],
    timeout: Duration,
) -> io::Result<(u8, usize)> {
    let opcode = u16::from_le_bytes([cmd[1], cmd[2]]);
    write_all_timeout(&ctrl, cmd, timeout)?;
    let deadline = Instant::now() + timeout;
    let mut d = Deframer::new(h4::FROM_CONTROLLER, h4::VHCI_MAX_FRAME);
    let mut buf = [0u8; READ_BUF];
    let mut other = 0;
    loop {
        while let Some(next) = d.next_packet() {
            if let Next::Packet(p) = next {
                if let Some(status) = h4::command_complete_status(&p, opcode) {
                    return Ok((status, other));
                }
                other += 1;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no Command Complete for {opcode:#06x}"),
            ));
        }
        if wait_readable(&ctrl, left)? {
            match read_some(&ctrl, &mut buf)? {
                Some(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "controller closed",
                    ))
                }
                Some(n) => d.push(&buf[..n]),
                None => {}
            }
        }
    }
}
