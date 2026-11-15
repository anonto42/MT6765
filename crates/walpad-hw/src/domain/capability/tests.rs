use super::*;

#[test]
fn capability_parsing_and_status() {
    assert_eq!(Capability::from_name("supported"), Capability::Supported);
    assert_eq!(Capability::from_name("YES"), Capability::Supported);
    assert_eq!(Capability::from_name("true"), Capability::Supported);
    assert_eq!(
        Capability::from_name("unsupported"),
        Capability::Unsupported
    );
    assert_eq!(Capability::from_name("false"), Capability::Unsupported);
    assert_eq!(Capability::from_name("disabled"), Capability::Unsupported);
    assert_eq!(Capability::from_name("invalid"), Capability::Unknown);

    assert!(Capability::Supported.is_supported());
    assert!(!Capability::Unsupported.is_supported());
    assert!(!Capability::Unknown.is_supported());

    assert_eq!(Capability::Supported.as_str(), "supported");
    assert_eq!(Capability::Unsupported.as_str(), "unsupported");
    assert_eq!(Capability::Unknown.as_str(), "unknown");
}
