//! walpad-btbridge: joins the MediaTek CONNSYS Bluetooth char device (`/dev/stpbt`, H:4 byte
//! stream, opened = BT function powered on) to the kernel's virtual HCI driver (`/dev/vhci`, one
//! packet per read/write), so BlueZ sees an ordinary `hci0`.
//!
//! Logging never contains packet payloads: only types, lengths and counters.

pub mod h4;

use h4::{Deframer, Next};
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
}

impl Stats {
    pub fn render(&self) -> String {
        format!(
            "to_host_packets={}\nto_host_bytes={}\nto_controller_packets={}\nto_controller_bytes={}\n\
             dropped_bytes={}\nrejected_host_packets={}\nhw_error_events={}\npatched_replies={}\n",
            self.to_host_packets,
            self.to_host_bytes,
            self.to_controller_packets,
            self.to_controller_bytes,
            self.dropped_bytes,
            self.rejected_host_packets,
            self.hw_error_events,
            self.patched_replies
        )
    }
}

fn timespec(d: Duration) -> Timespec {
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

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    Progress,
    Closed(&'static str),
}

/// Copies packets between the controller stream and the vhci packet device.
pub struct Bridge<C: AsFd, H: AsFd> {
    ctrl: C,
    host: H,
    deframer: Deframer,
    pub stats: Stats,
    pub write_timeout: Duration,
    buf: Vec<u8>,
}

impl<C: AsFd, H: AsFd> Bridge<C, H> {
    pub fn new(ctrl: C, host: H) -> Self {
        Bridge {
            ctrl,
            host,
            deframer: Deframer::new(h4::FROM_CONTROLLER, h4::VHCI_MAX_FRAME),
            stats: Stats::default(),
            write_timeout: Duration::from_secs(1),
            buf: vec![0u8; READ_BUF],
        }
    }

    /// Wait up to `timeout` for data on either side and move it.  `log` gets one line per event
    /// worth noting (never payload bytes).
    pub fn step(&mut self, timeout: Duration, log: &mut dyn FnMut(&str)) -> io::Result<Step> {
        let (cr, hr) = {
            let mut fds = [
                PollFd::new(&self.ctrl, PollFlags::IN),
                PollFd::new(&self.host, PollFlags::IN),
            ];
            match poll(&mut fds, Some(&timespec(timeout))) {
                Ok(0) | Err(Errno::INTR) => return Ok(Step::Idle),
                Ok(_) => {}
                Err(e) => return Err(e.into()),
            }
            (fds[0].revents(), fds[1].revents())
        };
        let gone = PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL;
        let mut moved = false;
        if cr.contains(PollFlags::IN) {
            moved |= self.controller_to_host(log)?;
        } else if cr.intersects(gone) {
            return Ok(Step::Closed("controller hung up"));
        }
        if hr.contains(PollFlags::IN) {
            moved |= self.host_to_controller(log)?;
        } else if hr.intersects(gone) {
            return Ok(Step::Closed("vhci hung up"));
        }
        Ok(if moved { Step::Progress } else { Step::Idle })
    }

    fn controller_to_host(&mut self, log: &mut dyn FnMut(&str)) -> io::Result<bool> {
        let n = match read_some(&self.ctrl, &mut self.buf)? {
            None => return Ok(false),
            Some(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "controller returned EOF",
                ))
            }
            Some(n) => n,
        };
        self.deframer.push(&self.buf[..n]);
        while let Some(next) = self.deframer.next_packet() {
            match next {
                Next::Packet(mut p) => {
                    if let Some(fixed) = h4::fixup_sync_train(&p) {
                        self.stats.patched_replies += 1;
                        log("controller rejected Read Sync Train Params; answering success (4.19 init would abort)");
                        p = fixed;
                    }
                    if p[0] == h4::EVENT && p[1] == 0x10 {
                        self.stats.hw_error_events += 1;
                        log("controller sent Hardware Error (chip reset?); forwarding to the host stack");
                    }
                    write_all_timeout(&self.host, &p, self.write_timeout).map_err(|e| {
                        io::Error::new(e.kind(), format!("vhci write ({} bytes): {e}", p.len()))
                    })?;
                    self.stats.to_host_packets += 1;
                    self.stats.to_host_bytes += p.len() as u64;
                }
                Next::Dropped(e) => {
                    self.stats.dropped_bytes += 1;
                    // log the first drop of a burst and then every 256th, not every byte
                    if self.stats.dropped_bytes % 256 == 1 {
                        log(&format!(
                            "resync from controller: {e} (dropped {} bytes so far)",
                            self.stats.dropped_bytes
                        ));
                    }
                }
            }
        }
        Ok(true)
    }

    fn host_to_controller(&mut self, log: &mut dyn FnMut(&str)) -> io::Result<bool> {
        let n = match read_some(&self.host, &mut self.buf)? {
            None => return Ok(false),
            Some(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "vhci returned EOF",
                ))
            }
            Some(n) => n,
        };
        let p = &self.buf[..n];
        if let Err(why) = h4::check_single(p, h4::FROM_HOST, h4::STPBT_MAX_WRITE) {
            self.stats.rejected_host_packets += 1;
            log(&format!(
                "dropping malformed packet from the host stack: {why}"
            ));
            return Ok(true);
        }
        if p[0] == h4::VENDOR {
            log("ignoring vhci vendor notice");
            return Ok(true);
        }
        write_all_timeout(&self.ctrl, p, self.write_timeout).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("stpbt write ({} {} bytes): {e}", h4::type_name(p[0]), n),
            )
        })?;
        self.stats.to_controller_packets += 1;
        self.stats.to_controller_bytes += n as u64;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixDatagram, UnixStream};

    fn pair_stream() -> (UnixStream, UnixStream) {
        let (a, b) = UnixStream::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        (a, b)
    }
    fn pair_dgram() -> (UnixDatagram, UnixDatagram) {
        let (a, b) = UnixDatagram::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        b.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        (a, b)
    }
    fn run(b: &mut Bridge<UnixStream, UnixDatagram>, steps: usize) -> Vec<String> {
        let mut logs = Vec::new();
        for _ in 0..steps {
            b.step(Duration::from_millis(50), &mut |l| logs.push(l.to_string()))
                .unwrap();
        }
        logs
    }

    #[test]
    fn controller_stream_is_reframed_one_packet_per_vhci_write() {
        let (ctrl, mut ctrl_peer) = pair_stream();
        let (host, host_peer) = pair_dgram();
        let mut b = Bridge::new(ctrl, host);
        let ev = [4u8, 0x0e, 4, 1, 3, 0x0c, 0];
        let acl = [2u8, 1, 0x20, 3, 0, 9, 8, 7];
        // two packets merged, the second split over two writes
        ctrl_peer.write_all(&[&ev[..], &acl[..2]].concat()).unwrap();
        run(&mut b, 2);
        ctrl_peer.write_all(&acl[2..]).unwrap();
        run(&mut b, 2);
        let mut buf = [0u8; 64];
        let n = host_peer.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], &ev);
        let n = host_peer.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], &acl);
        assert_eq!(b.stats.to_host_packets, 2);
        assert_eq!(b.stats.to_host_bytes, (ev.len() + acl.len()) as u64);
    }

    #[test]
    fn host_packets_go_to_controller_and_garbage_is_rejected() {
        let (ctrl, mut ctrl_peer) = pair_stream();
        let (host, host_peer) = pair_dgram();
        ctrl_peer
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut b = Bridge::new(ctrl, host);
        let reset = [1u8, 0x03, 0x0c, 0];
        host_peer.send(&[1u8, 0x03, 0x0c, 5]).unwrap(); // truncated command
        host_peer.send(&reset).unwrap();
        let logs = run(&mut b, 3);
        let mut got = [0u8; 16];
        let n = ctrl_peer.read(&mut got).unwrap();
        assert_eq!(&got[..n], &reset);
        assert_eq!(b.stats.rejected_host_packets, 1);
        assert_eq!(b.stats.to_controller_packets, 1);
        assert!(logs.iter().any(|l| l.contains("malformed")));
        // logs carry no payload bytes
        assert!(logs
            .iter()
            .all(|l| !l.contains("0x0c03") && !l.contains("03 0c")));
    }

    #[test]
    fn hardware_error_is_counted_and_forwarded() {
        let (ctrl, mut ctrl_peer) = pair_stream();
        let (host, host_peer) = pair_dgram();
        let mut b = Bridge::new(ctrl, host);
        ctrl_peer.write_all(&[4, 0x10, 1, 0]).unwrap();
        let logs = run(&mut b, 1);
        let mut buf = [0u8; 8];
        assert_eq!(host_peer.recv(&mut buf).unwrap(), 4);
        assert_eq!(b.stats.hw_error_events, 1);
        assert!(logs[0].contains("Hardware Error"));
    }

    #[test]
    fn controller_hangup_is_reported() {
        let (ctrl, ctrl_peer) = pair_stream();
        let (host, _host_peer) = pair_dgram();
        let mut b = Bridge::new(ctrl, host);
        drop(ctrl_peer);
        // a closed stream polls readable and reads 0 -> EOF error
        let r = b.step(Duration::from_millis(50), &mut |_| {});
        assert!(r.is_err() || r.as_ref().ok() == Some(&Step::Closed("controller hung up")));
    }

    #[test]
    fn create_hci_reads_the_index() {
        let (vhci, peer) = pair_dgram();
        peer.send(&[0xff, 0x00, 0x02, 0x00]).unwrap();
        assert_eq!(create_hci(&vhci, Duration::from_secs(1)).unwrap(), 2);
        let mut req = [0u8; 4];
        assert_eq!(peer.recv(&mut req).unwrap(), 2);
        assert_eq!(&req[..2], &[0xff, 0x00]);
    }

    #[test]
    fn create_hci_times_out() {
        let (vhci, _peer) = pair_dgram();
        let e = create_hci(&vhci, Duration::from_millis(100)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn controller_command_waits_for_its_complete() {
        let (ctrl, mut peer) = pair_stream();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        // an unrelated event first, then the right Command Complete split in two writes
        peer.write_all(&[4, 0x0e, 4, 1, 0x03, 0x0c, 0, 4, 0x0e])
            .unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            peer.write_all(&[4, 1, 0x1a, 0xfc, 0]).unwrap();
            let mut got = [0u8; 16];
            let n = peer.read(&mut got).unwrap();
            got[..n].to_vec()
        });
        let cmd = h4::set_bdaddr_cmd([1, 2, 3, 4, 5, 6]);
        let (status, other) = controller_command(&ctrl, &cmd, Duration::from_secs(2)).unwrap();
        assert_eq!((status, other), (0, 1));
        assert_eq!(t.join().unwrap(), cmd);
    }

    #[test]
    fn write_times_out_when_peer_never_reads() {
        let (a, _b) = pair_stream();
        let big = vec![0u8; 8 << 20];
        let e = write_all_timeout(&a, &big, Duration::from_millis(100)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    }
}
