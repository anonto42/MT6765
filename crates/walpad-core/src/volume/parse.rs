use std::process::Child;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Job {
    Get,
    Apply,
}

pub(super) struct Run {
    pub(super) job: Job,
    pub(super) child: Child,
    pub(super) deadline: Instant,
}

/// One `key=value` line of the script's output.
pub(super) fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
}

/// `<percent> <muted>` of the state file.
pub fn parse_state(text: &str) -> Option<(u8, bool)> {
    let mut it = text.split_whitespace();
    let v: u32 = it.next()?.parse().ok()?;
    let m = it.next().map(|m| m == "1").unwrap_or(false);
    (v <= 100).then_some((v as u8, m))
}
