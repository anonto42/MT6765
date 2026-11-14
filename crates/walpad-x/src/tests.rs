use super::*;
use std::time::{Duration, Instant};

#[test]
fn connecting_to_a_missing_server_fails_fast() {
    let t = Instant::now();
    assert!(connect(Path::new("/tmp/.X11-unix/X998")).is_err());
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[test]
fn wait_any_reports_which_socket_is_ready() {
    use std::io::Write;
    let (a, mut a2) = UnixStream::pair().unwrap();
    let (b, _b2) = UnixStream::pair().unwrap();
    assert_eq!(wait_any(&[&a, &b], Some(20)), vec![false, false]);
    a2.write_all(b"x").unwrap();
    assert_eq!(wait_any(&[&a, &b], Some(200)), vec![true, false]);
}

#[test]
fn wait_times_out_on_a_quiet_socket() {
    let (a, _b) = UnixStream::pair().unwrap();
    let t = Instant::now();
    assert!(!wait_readable(&a, Some(50)));
    assert!(t.elapsed() >= Duration::from_millis(40));
}
