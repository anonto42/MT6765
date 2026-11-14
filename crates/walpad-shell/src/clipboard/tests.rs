use super::*;

#[test]
fn history_is_newest_first_deduplicated_and_capped() {
    let mut h = History::default();
    for i in 0..12 {
        assert!(h.push(format!("entry {i}")));
    }
    assert_eq!(h.len(), HISTORY_MAX);
    assert_eq!(h.get(0), Some("entry 11"));
    assert_eq!(h.get(9), Some("entry 2"));
    // a repeat moves to the top, no duplicate
    assert!(h.push("entry 5".into()));
    assert_eq!(h.len(), HISTORY_MAX);
    assert_eq!(h.get(0), Some("entry 5"));
    assert_eq!(h.iter().filter(|t| *t == "entry 5").count(), 1);
}

#[test]
fn history_refuses_blank_huge_and_binary_text() {
    let mut h = History::default();
    assert!(!h.push(String::new()));
    assert!(!h.push(" \n\t".into()));
    assert!(!h.push("a\0b".into()));
    assert!(!h.push("x".repeat(ENTRY_MAX + 1)));
    assert!(h.push("x".repeat(ENTRY_MAX)));
    assert_eq!(h.len(), 1);
}

#[test]
fn promote_moves_an_entry_to_the_top() {
    let mut h = History::default();
    for t in ["a", "b", "c"] {
        h.push(t.into());
    }
    assert_eq!(h.promote(2), Some("a"));
    assert_eq!(h.iter().cloned().collect::<Vec<_>>(), ["a", "c", "b"]);
    assert_eq!(h.promote(9), None);
    h.clear();
    assert!(h.is_empty());
}
