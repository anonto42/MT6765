//! The binary's `--capabilities` output must contain what walpad-runtime requires for WALPAD_SHELL.
use std::path::Path;
use std::process::Command;

#[test]
fn shell_capability_matches_walpad_runtime() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/rootfs/usr/local/sbin/walpad-runtime");
    let text = std::fs::read_to_string(&script).unwrap();
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("WALPAD_SHELL)"))
        .expect("WALPAD_SHELL case in walpad-runtime");
    let cap = line
        .split("cap=")
        .nth(1)
        .expect("cap= in the WALPAD_SHELL case")
        .trim_end_matches(';')
        .trim();
    assert_eq!(cap, "shell");
    let out = Command::new(env!("CARGO_BIN_EXE_walpad-shell"))
        .arg("--capabilities")
        .output()
        .unwrap();
    let got = String::from_utf8(out.stdout).unwrap();
    assert!(got.split_whitespace().any(|c| c == cap), "got {got:?}");
}
