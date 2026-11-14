pub struct FakeAp {
    pub ssid: Vec<u8>,
    pub signal: i32,
    pub freq: u32,
    pub flags: String,
    /// Passphrase of the AP (None = open).
    pub passphrase: Option<String>,
}

pub(super) struct Net {
    pub(super) id: u32,
    pub(super) ssid_hex: String,
    pub(super) psk: Option<String>,
    pub(super) open: bool,
    pub(super) enabled: bool,
}

pub fn default_aps() -> Vec<FakeAp> {
    let ap = |s: &str, sig, freq, flags: &str, pw: Option<&str>| FakeAp {
        ssid: s.as_bytes().to_vec(),
        signal: sig,
        freq,
        flags: flags.into(),
        passphrase: pw.map(String::from),
    };
    vec![
        ap(
            "Home",
            -52,
            5180,
            "[WPA2-PSK-CCMP][ESS]",
            Some("correct horse"),
        ),
        ap(
            "Home",
            -70,
            2412,
            "[WPA2-PSK-CCMP][ESS]",
            Some("correct horse"),
        ),
        ap("Cafe Open", -60, 2437, "[ESS]", None),
        FakeAp {
            ssid: "Café".as_bytes().to_vec(),
            signal: -75,
            freq: 2462,
            flags: "[WPA2-PSK-CCMP][ESS]".into(),
            passphrase: Some("espresso123".into()),
        },
    ]
}
