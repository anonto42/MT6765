use super::*;

#[test]
fn parse_and_name_round_trip() {
    for k in [JackKind::Headphones, JackKind::HeadsetWithMic] {
        assert_eq!(parse_kind(kind_name(k)), Some(k));
    }
    assert_eq!(parse_kind("none"), None);
    assert_eq!(parse_kind(""), None);
    assert_eq!(parse_kind("garbage"), None);
}

#[test]
fn new_reads_the_persisted_override_and_applies_once() {
    let dir = std::env::temp_dir().join(format!("walpad-jack-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("var/lib/walpad")).unwrap();
    std::fs::write(dir.join("var/lib/walpad/jack-force"), "headset").unwrap();
    let root = Root::new(dir.clone());
    let now = Instant::now();
    let j = Jack::new(&root, now);
    assert_eq!(j.status().forced, Some(JackKind::HeadsetWithMic));
    assert_eq!(j.status().route, Some(JackKind::HeadsetWithMic));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_force_persists_and_clears() {
    let dir = std::env::temp_dir().join(format!("walpad-jack-test2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let root = Root::new(dir.clone());
    let mut j = Jack::new(&root, Instant::now());
    let st = j.set_force(Some(JackKind::Headphones));
    assert_eq!(st.route, Some(JackKind::Headphones));
    assert_eq!(
        std::fs::read_to_string(dir.join("var/lib/walpad/jack-force")).unwrap(),
        "headphones"
    );
    let st = j.set_force(None);
    assert_eq!(st.route, None);
    assert_eq!(
        std::fs::read_to_string(dir.join("var/lib/walpad/jack-force")).unwrap(),
        "none"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
