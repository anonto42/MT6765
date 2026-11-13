use std::fs::File;
use std::io::Read;
use std::time::Duration;

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;
pub const EV_SW: u16 = 5;
pub const SYN_REPORT: u16 = 0;
/// Multitouch protocol A: ends one contact inside a frame.
pub const SYN_MT_REPORT: u16 = 2;
pub const BTN_TOUCH: u16 = 0x14a;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;
pub const KEY_POWER: u16 = 116;
pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;
pub const ABS_MT_POSITION_X: u16 = 53;
pub const ABS_MT_POSITION_Y: u16 = 54;
pub const SW_HEADPHONE_INSERT: u16 = 2;
pub const SW_MICROPHONE_INSERT: u16 = 4;
pub const SW_LINEOUT_INSERT: u16 = 6;

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

/// EVIOCGKEY(96): `_IOC(_IOC_READ, 'E', 0x18, 96)`.  Same value on aarch64 and x86_64 (the size
/// field is the bitmap length, the direction bits are the generic read = 2).
pub const EVIOCGKEY_96: u32 = 0x8060_4518;
/// Bytes of the global key-state bitmap we ask for (768 keys; KEY_MAX is 0x2ff).
pub const KEY_BITMAP_BYTES: usize = 96;
/// EVIOCGSW(8): `_IOC(_IOC_READ, 'E', 0x1b, 8)`.  Same value on aarch64 and x86_64.
pub const EVIOCGSW_8: u32 = 0x8008_451b;
/// Bytes of the global switch-state bitmap we ask for (64 switches; SW_MAX is 0x0f).
pub const SW_BITMAP_BYTES: usize = 8;

/// Is `code` set in an EVIOCGKEY/EVIOCGSW bitmap (bit `code % 8` of byte `code / 8`)?  A code
/// beyond the buffer is "not held".
pub fn key_bit(bitmap: &[u8], code: u16) -> bool {
    bitmap
        .get(usize::from(code) / 8)
        .is_some_and(|b| b & (1u8 << (code % 8)) != 0)
}

struct GetBitmap<const N: usize, const OPCODE: u32> {
    buf: [u8; N],
}
// SAFETY: callers instantiate GetBitmap only with an EVIOCG* read opcode whose encoded byte
// length equals N, so the kernel writes at most N bytes at `buf`.
unsafe impl<const N: usize, const OPCODE: u32> rustix::ioctl::Ioctl for GetBitmap<N, OPCODE> {
    type Output = [u8; N];
    const IS_MUTATING: bool = true;
    fn opcode(&self) -> rustix::ioctl::Opcode {
        OPCODE as _
    }
    fn as_ptr(&mut self) -> *mut std::ffi::c_void {
        self.buf.as_mut_ptr().cast()
    }
    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        p: *mut std::ffi::c_void,
    ) -> rustix::io::Result<Self::Output> {
        // SAFETY: p points at our N-byte array.
        Ok(unsafe { *(p as *const [u8; N]) })
    }
}

/// SAFETY: `OPCODE` must be a valid `EVIOCG*` read ioctl whose encoded buffer size equals `N`.
unsafe fn read_bitmap<const N: usize, const OPCODE: u32>(
    path: &std::path::Path,
) -> std::io::Result<[u8; N]> {
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)?;
    if f.metadata()?.file_type().is_char_device() {
        // SAFETY: caller guarantees OPCODE is an EVIOCG* read ioctl of size N.
        unsafe { rustix::ioctl::ioctl(&f, GetBitmap::<N, OPCODE> { buf: [0; N] }) }
            .map_err(std::io::Error::from)
    } else {
        let mut b = [0u8; N];
        let _ = f.read(&mut b)?;
        Ok(b)
    }
}

/// Current key state of an evdev node.  Never blocks (O_NONBLOCK, ioctl only).  On a character
/// device this is EVIOCGKEY (CANNOT be exercised on the host: verified only on the tablet); on a
/// plain file (tests) the first 96 bytes are the bitmap.
pub fn read_key_bitmap(path: &std::path::Path) -> std::io::Result<[u8; KEY_BITMAP_BYTES]> {
    // SAFETY: EVIOCGKEY_96 encodes a 96-byte read buffer, matching KEY_BITMAP_BYTES.
    unsafe { read_bitmap::<KEY_BITMAP_BYTES, EVIOCGKEY_96>(path) }
}

/// Current switch (`EV_SW`) state of an evdev node.  Never blocks (O_NONBLOCK, ioctl only).
/// On a plain file (tests) the first 8 bytes are the bitmap.
pub fn read_sw_bitmap(path: &std::path::Path) -> std::io::Result<[u8; SW_BITMAP_BYTES]> {
    // SAFETY: EVIOCGSW_8 encodes an 8-byte read buffer, matching SW_BITMAP_BYTES.
    unsafe { read_bitmap::<SW_BITMAP_BYTES, EVIOCGSW_8>(path) }
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
    fn key_bitmap_bits() {
        let mut b = [0u8; KEY_BITMAP_BYTES];
        assert!(!key_bit(&b, KEY_VOLUMEDOWN));
        b[14] = 1 << 2; // 114 = 14 * 8 + 2
        assert!(key_bit(&b, KEY_VOLUMEDOWN));
        assert!(!key_bit(&b, KEY_VOLUMEUP) && !key_bit(&b, KEY_POWER));
        assert!(!key_bit(&b, 1000)); // beyond the buffer
        assert!(!key_bit(&[], 0));
        b[95] = 0x80; // code 767
        assert!(key_bit(&b, 767));
        assert_eq!(EVIOCGKEY_96, (2 << 30) | (96 << 16) | (0x45 << 8) | 0x18);
    }

    #[test]
    fn key_bitmap_from_plain_file() {
        let p = crate::testutil::tmp().join("kpd");
        let mut b = vec![0u8; KEY_BITMAP_BYTES];
        b[14] = 4;
        std::fs::write(&p, &b).unwrap();
        let got = read_key_bitmap(&p).unwrap();
        assert!(key_bit(&got, 114));
        assert!(read_key_bitmap(&p.with_extension("none")).is_err());
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
