use crate::Root;
use std::ffi::c_void;
use std::io::Read;
use std::os::unix::fs::FileTypeExt;

pub const GSENSOR_IOCTL: u32 = 0x8004_8503;
const BUF: usize = 128;

/// Parse the gsensor string: three hex words "x y z", each a signed 32-bit value.
pub fn parse_gsensor(s: &str) -> Option<(i32, i32, i32)> {
    let s = s.split('\0').next().unwrap_or("");
    let mut it = s.split_whitespace().map(parse_word);
    let (x, y, z) = (it.next()??, it.next()??, it.next()??);
    Some((x, y, z))
}

fn parse_word(w: &str) -> Option<i32> {
    let (neg, w) = match w.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, w),
    };
    let w = w.trim_start_matches("0x").trim_start_matches("0X");
    let v = u32::from_str_radix(w, 16).ok()? as i32;
    Some(if neg { v.wrapping_neg() } else { v })
}

struct GsIoctl {
    buf: [u8; BUF],
}
// SAFETY: the opcode is the vendor's read ioctl that fills at most 128 bytes at the pointer.
unsafe impl rustix::ioctl::Ioctl for GsIoctl {
    type Output = [u8; BUF];
    const IS_MUTATING: bool = true;
    fn opcode(&self) -> rustix::ioctl::Opcode {
        GSENSOR_IOCTL as _
    }
    fn as_ptr(&mut self) -> *mut c_void {
        self.buf.as_mut_ptr().cast()
    }
    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        p: *mut c_void,
    ) -> rustix::io::Result<Self::Output> {
        // SAFETY: p points at our BUF-byte array.
        Ok(unsafe { *(p as *const [u8; BUF]) })
    }
}

/// Read one sample. On the device /dev/gsensor is a character device read through the
/// ioctl; under a test root it is a plain file holding the string.
pub fn read_gsensor(root: &Root) -> Option<(i32, i32, i32)> {
    let path = root.p("/dev/gsensor");
    let meta = std::fs::metadata(&path).ok()?;
    if meta.file_type().is_char_device() {
        let f = std::fs::File::open(&path).ok()?;
        // SAFETY: GsIoctl matches the driver's ABI (see above).
        let out = unsafe { rustix::ioctl::ioctl(&f, GsIoctl { buf: [0; BUF] }) }.ok()?;
        parse_gsensor(&String::from_utf8_lossy(&out))
    } else {
        let mut s = String::new();
        std::fs::File::open(&path)
            .ok()?
            .read_to_string(&mut s)
            .ok()?;
        parse_gsensor(&s)
    }
}

#[cfg(test)]
mod tests;
