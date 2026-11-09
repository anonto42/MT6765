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
mod tests {
    use super::*;

    #[test]
    fn matches_ieee_802_11i_vectors() {
        // wpa_passphrase IEEE password
        assert_eq!(
            hex(&derive("password", b"IEEE")),
            "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
        );
        // wpa_passphrase ThisIsASSID ThisIsAPassword
        assert_eq!(
            hex(&derive("ThisIsAPassword", b"ThisIsASSID")),
            "0dc0d6eb90555ed6419756b9a15ec3e3209b63df707dd508d14581f8982721af"
        );
    }

    #[test]
    fn secret_forms() {
        assert_eq!(
            psk_hex("password", b"IEEE").unwrap(),
            "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
        );
        let raw = "F42C6FC52DF0EBEF9EBB4B90B38A5F902E83FE1B135A70E23AED762E9710A12E";
        assert_eq!(psk_hex(raw, b"x").unwrap(), raw.to_lowercase());
        assert_eq!(psk_hex("12345", b"x"), Err(PskError::BadLength));
        assert_eq!(psk_hex("1234567", b"x"), Err(PskError::BadLength));
        // 13 characters is a valid WPA passphrase (not a WEP-104 key)
        assert!(psk_hex("correct horse", b"Home").is_ok());
        assert_eq!(psk_hex(&"z".repeat(64), b"x"), Err(PskError::BadLength));
    }
}
