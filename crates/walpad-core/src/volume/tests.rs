use super::parse::field;
use super::*;

#[test]
fn state_file_parsing() {
    assert_eq!(parse_state("70 0\n"), Some((70, false)));
    assert_eq!(parse_state("0 1"), Some((0, true)));
    assert_eq!(parse_state("55"), Some((55, false)));
    assert_eq!(parse_state("101 0"), None);
    assert_eq!(parse_state("x"), None);
    assert_eq!(parse_state(""), None);
}

#[test]
fn output_fields() {
    let o = "volume=40\nmuted=1\navailable=1\nbackend=pulse\n";
    assert_eq!(field(o, "volume"), Some("40"));
    assert_eq!(field(o, "backend"), Some("pulse"));
    assert_eq!(field(o, "nope"), None);
}
