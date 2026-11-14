/// The GSM 7-bit default alphabet; index = septet value. 0x1B is the escape to the extension table.
const GSM7: [char; 128] = [
    '@', '£', '$', '¥', 'è', 'é', 'ù', 'ì', 'ò', 'Ç', '\n', 'Ø', 'ø', '\r', 'Å', 'å', 'Δ', '_',
    'Φ', 'Γ', 'Λ', 'Ω', 'Π', 'Ψ', 'Σ', 'Θ', 'Ξ', '\u{1b}', 'Æ', 'æ', 'ß', 'É', ' ', '!', '"', '#',
    '¤', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', '0', '1', '2', '3', '4', '5', '6',
    '7', '8', '9', ':', ';', '<', '=', '>', '?', '¡', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I',
    'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'Ä', 'Ö',
    'Ñ', 'Ü', '§', '¿', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o',
    'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'ä', 'ö', 'ñ', 'ü', 'à',
];

/// The extension table (after 0x1B): (septet, char).
const EXT: [(u8, char); 10] = [
    (0x0A, '\u{0c}'),
    (0x14, '^'),
    (0x28, '{'),
    (0x29, '}'),
    (0x2F, '\\'),
    (0x3C, '['),
    (0x3D, '~'),
    (0x3E, ']'),
    (0x40, '|'),
    (0x65, '€'),
];

/// Longest message we send: this many parts (6 x 153 septets or 6 x 67 UCS2 units).
pub const MAX_PARTS: usize = 6;
pub const SINGLE_GSM7: usize = 160;
pub const SINGLE_UCS2: usize = 70;
pub const PART_GSM7: usize = 153;
pub const PART_UCS2: usize = 67;

/// The septets of `text`, None when a character is not in the GSM 7-bit alphabet.
pub fn gsm7_encode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        if let Some(i) = GSM7.iter().position(|g| *g == c).filter(|i| *i != 0x1B) {
            out.push(i as u8);
        } else if let Some((s, _)) = EXT.iter().find(|(_, e)| *e == c) {
            out.push(0x1B);
            out.push(*s);
        } else {
            return None;
        }
    }
    Some(out)
}

/// Septets back to text; unknown septets after an escape become a space, like phones do.
pub fn gsm7_decode(septets: &[u8]) -> String {
    let mut out = String::new();
    let mut it = septets.iter();
    while let Some(&s) = it.next() {
        if s == 0x1B {
            match it.next() {
                Some(&e) => out.push(EXT.iter().find(|(k, _)| *k == e).map_or(' ', |(_, c)| *c)),
                None => break,
            }
        } else {
            out.push(GSM7[(s & 0x7F) as usize]);
        }
    }
    out
}

/// Pack septets into octets, starting `pad` bits into the first octet (UDH alignment).
pub fn pack7(septets: &[u8], pad: usize) -> Vec<u8> {
    let total_bits = pad + septets.len() * 7;
    let mut out = vec![0u8; total_bits.div_ceil(8)];
    for (i, s) in septets.iter().enumerate() {
        let bit = pad + i * 7;
        let (byte, off) = (bit / 8, bit % 8);
        out[byte] |= s << off;
        if off > 1 {
            out[byte + 1] |= s >> (8 - off);
        }
    }
    out
}

/// Unpack `count` septets that start `skip` bits into `bytes`.
pub fn unpack7(bytes: &[u8], count: usize, skip: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bit = skip + i * 7;
        let (byte, off) = (bit / 8, bit % 8);
        let lo = bytes.get(byte).copied().unwrap_or(0) as u16;
        let hi = bytes.get(byte + 1).copied().unwrap_or(0) as u16;
        out.push((((hi << 8 | lo) >> off) & 0x7F) as u8);
    }
    out
}

/// UCS2 is UTF-16 big endian.
pub fn ucs2_encode(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
}

pub fn ucs2_decode(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Semi-octet (swapped BCD) digits of a phone number; `*`, `#` and friends use 0xA-0xE.
pub(super) fn bcd_encode(digits: &str) -> Option<Vec<u8>> {
    let mut nib: Vec<u8> = Vec::with_capacity(digits.len() + 1);
    for c in digits.chars() {
        nib.push(match c {
            '0'..='9' => c as u8 - b'0',
            '*' => 0xA,
            '#' => 0xB,
            _ => return None,
        });
    }
    if nib.len() % 2 == 1 {
        nib.push(0xF);
    }
    Some(nib.chunks(2).map(|p| p[0] | (p[1] << 4)).collect())
}

pub(super) fn bcd_decode(bytes: &[u8], digits: usize) -> String {
    let mut s = String::new();
    for b in bytes {
        for n in [b & 0x0F, b >> 4] {
            if s.chars().count() >= digits {
                break;
            }
            s.push(match n {
                0..=9 => (b'0' + n) as char,
                0xA => '*',
                0xB => '#',
                0xC => 'a',
                0xD => 'b',
                0xE => 'c',
                _ => continue,
            });
        }
    }
    s
}
