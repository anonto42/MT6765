use crate::h4::{self, Deframer, Next};
use crate::io::{read_some, timespec, write_all_timeout, Stats, READ_BUF};
use rustix::event::{poll, PollFd, PollFlags};
use rustix::io::Errno;
use std::io;
use std::os::fd::AsFd;
use std::time::Duration;

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
