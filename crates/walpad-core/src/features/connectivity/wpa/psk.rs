//! WPA2-PSK key derivation. The passphrase is turned into the 64-hex PSK here, so only the
//! derived key (never the passphrase) is sent to wpa_supplicant and nothing secret reaches argv.
use super::parse::hex;
use sha1::Sha1;

/// PBKDF2-HMAC-SHA1, 4096 rounds, 32 bytes, SSID bytes as salt (IEEE 802.11i).
pub fn derive(passphrase: &str, ssid: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha1>(passphrase.as_bytes(), ssid, 4096, &mut out);
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum PskError {
    /// Not 8-63 characters and not a 64-digit hex key. (WEP is unsupported; its 5/13-character
    /// keys cannot be told apart from short/valid passphrases, so no length is treated as WEP.)
    BadLength,
}

/// Accepts a passphrase of 8-63 characters or an already derived 64-hex PSK.
pub fn psk_hex(secret: &str, ssid: &[u8]) -> Result<String, PskError> {
    match secret.len() {
        64 if secret.bytes().all(|b| b.is_ascii_hexdigit()) => Ok(secret.to_ascii_lowercase()),
        8..=63 => Ok(hex(&derive(secret, ssid))),
        _ => Err(PskError::BadLength),
    }
}

#[cfg(test)]
mod tests;
