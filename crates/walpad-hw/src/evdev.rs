use std::fs::File;
use std::io::Read;
use std::time::Duration;

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;
pub const EV_SW: u16 = 5;
pub const SYN_REPORT: u16 = 0;
pub const BTN_TOUCH: u16 = 0x14a;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;
pub const KEY_POWER: u16 = 116;
pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;
pub const ABS_MT_POSITION_X: u16 = 53;
pub const ABS_MT_POSITION_Y: u16 = 54;

/// aarch64 `struct input_event`: tv_sec i64, tv_usec i64, type u16, code u16, value i32.
pub const EVENT_SIZE: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub sec: i64,
    pub usec: i64,
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl InputEvent {
    pub fn ms(&self) -> i64 {
        self.sec * 1000 + self.usec / 1000
    }
    pub fn to_bytes(&self) -> [u8; EVENT_SIZE] {
        let mut b = [0u8; EVENT_SIZE];
        b[0..8].copy_from_slice(&self.sec.to_le_bytes());
        b[8..16].copy_from_slice(&self.usec.to_le_bytes());
        b[16..18].copy_from_slice(&self.kind.to_le_bytes());
        b[18..20].copy_from_slice(&self.code.to_le_bytes());
        b[20..24].copy_from_slice(&self.value.to_le_bytes());
        b
    }
}

/// Decode whole 24-byte records; a trailing partial record is ignored.
pub fn parse_events(data: &[u8]) -> Vec<InputEvent> {
    data.as_chunks::<EVENT_SIZE>()
        .0
        .iter()
        .filter_map(|c| {
            Some(InputEvent {
                sec: i64::from_le_bytes(c[0..8].try_into().ok()?),
                usec: i64::from_le_bytes(c[8..16].try_into().ok()?),
                kind: u16::from_le_bytes(c[16..18].try_into().ok()?),
                code: u16::from_le_bytes(c[18..20].try_into().ok()?),
                value: i32::from_le_bytes(c[20..24].try_into().ok()?),
            })
        })
        .collect()
}

/// Reads events from an evdev node (or a recorded .bin). Never grabs the device.
pub struct EvdevReader {
    file: File,
    pending: Vec<u8>,
}

impl EvdevReader {
    pub fn open(path: &std::path::Path) -> std::io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
            .open(path)?;
        Ok(EvdevReader {
            file,
            pending: Vec::new(),
        })
    }
    /// Read whatever is available now (non-blocking).
    pub fn read_available(&mut self) -> std::io::Result<Vec<InputEvent>> {
        let mut buf = [0u8; EVENT_SIZE * 64];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        let whole = self.pending.len() / EVENT_SIZE * EVENT_SIZE;
        let ev = parse_events(&self.pending[..whole]);
        self.pending.drain(..whole);
        Ok(ev)
    }
}

/// Wait up to `timeout` for any of the readers to become readable. Returns indices.
pub fn wait_readable(readers: &[&EvdevReader], timeout: Duration) -> Vec<usize> {
    use rustix::event::{poll, PollFd, PollFlags};
    let mut fds: Vec<PollFd<'_>> = readers
        .iter()
        .map(|r| PollFd::new(&r.file, PollFlags::IN))
        .collect();
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let t = rustix::event::Timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) as i64) * 1_000_000,
    };
    if poll(&mut fds, Some(&t)).is_err() {
        return Vec::new();
    }
    fds.iter()
        .enumerate()
        .filter(|(_, f)| f.revents().contains(PollFlags::IN))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            sec: 10,
            usec: 500_000,
            kind,
            code,
            value,
        }
    }

    #[test]
    fn roundtrip_and_partial() {
        let a = ev(EV_ABS, ABS_MT_POSITION_X, 400);
        let b = ev(EV_SYN, SYN_REPORT, 0);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&a.to_bytes());
        bytes.extend_from_slice(&b.to_bytes());
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(parse_events(&bytes), vec![a, b]);
        assert_eq!(a.ms(), 10_500);
        assert_eq!(EVENT_SIZE, 24);
    }

    #[test]
    fn reader_reads_file_in_chunks() {
        let dir = crate::testutil::tmp();
        let p = dir.join("event.bin");
        let a = ev(EV_KEY, KEY_POWER, 1);
        let mut data = a.to_bytes().to_vec();
        data.extend_from_slice(&a.to_bytes()[..10]);
        std::fs::write(&p, data).unwrap();
        let mut r = EvdevReader::open(&p).unwrap();
        assert_eq!(r.read_available().unwrap(), vec![a]);
        assert_eq!(wait_readable(&[&r], Duration::from_millis(10)).len(), 1);
    }
}
