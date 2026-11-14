use super::*;
use walpad_proto::WifiRadio;

#[test]
fn wifi_state_parsing() {
    assert_eq!(parse_wifi_state(None).radio, WifiRadio::Down);
    assert_eq!(parse_wifi_state(Some("failed\n")).radio, WifiRadio::Failed);
    assert_eq!(parse_wifi_state(Some("started")).radio, WifiRadio::Starting);
    let w = parse_wifi_state(Some("COMPLETED 192.168.0.100"));
    assert_eq!(
        (w.radio, w.state.as_deref(), w.ip.as_deref()),
        (WifiRadio::Up, Some("COMPLETED"), Some("192.168.0.100"))
    );
    assert_eq!(parse_wifi_state(Some("SCANNING")).ip, None);
}
