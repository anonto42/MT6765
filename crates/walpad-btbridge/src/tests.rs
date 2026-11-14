use super::*;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::time::Duration;

fn pair_stream() -> (UnixStream, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    a.set_nonblocking(true).unwrap();
    (a, b)
}
fn pair_dgram() -> (UnixDatagram, UnixDatagram) {
    let (a, b) = UnixDatagram::pair().unwrap();
    a.set_nonblocking(true).unwrap();
    b.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    (a, b)
}
fn run(b: &mut Bridge<UnixStream, UnixDatagram>, steps: usize) -> Vec<String> {
    let mut logs = Vec::new();
    for _ in 0..steps {
        b.step(Duration::from_millis(50), &mut |l| logs.push(l.to_string()))
            .unwrap();
    }
    logs
}

#[test]
fn controller_stream_is_reframed_one_packet_per_vhci_write() {
    let (ctrl, mut ctrl_peer) = pair_stream();
    let (host, host_peer) = pair_dgram();
    let mut b = Bridge::new(ctrl, host);
    let ev = [4u8, 0x0e, 4, 1, 3, 0x0c, 0];
    let acl = [2u8, 1, 0x20, 3, 0, 9, 8, 7];
    // two packets merged, the second split over two writes
    ctrl_peer.write_all(&[&ev[..], &acl[..2]].concat()).unwrap();
    run(&mut b, 2);
    ctrl_peer.write_all(&acl[2..]).unwrap();
    run(&mut b, 2);
    let mut buf = [0u8; 64];
    let n = host_peer.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], &ev);
    let n = host_peer.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], &acl);
    assert_eq!(b.stats.to_host_packets, 2);
    assert_eq!(b.stats.to_host_bytes, (ev.len() + acl.len()) as u64);
}

#[test]
fn host_packets_go_to_controller_and_garbage_is_rejected() {
    let (ctrl, mut ctrl_peer) = pair_stream();
    let (host, host_peer) = pair_dgram();
    ctrl_peer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut b = Bridge::new(ctrl, host);
    let reset = [1u8, 0x03, 0x0c, 0];
    host_peer.send(&[1u8, 0x03, 0x0c, 5]).unwrap(); // truncated command
    host_peer.send(&reset).unwrap();
    let logs = run(&mut b, 3);
    let mut got = [0u8; 16];
    let n = ctrl_peer.read(&mut got).unwrap();
    assert_eq!(&got[..n], &reset);
    assert_eq!(b.stats.rejected_host_packets, 1);
    assert_eq!(b.stats.to_controller_packets, 1);
    assert!(logs.iter().any(|l| l.contains("malformed")));
    // logs carry no payload bytes
    assert!(logs
        .iter()
        .all(|l| !l.contains("0x0c03") && !l.contains("03 0c")));
}

#[test]
fn hardware_error_is_counted_and_forwarded() {
    let (ctrl, mut ctrl_peer) = pair_stream();
    let (host, host_peer) = pair_dgram();
    let mut b = Bridge::new(ctrl, host);
    ctrl_peer.write_all(&[4, 0x10, 1, 0]).unwrap();
    let logs = run(&mut b, 1);
    let mut buf = [0u8; 8];
    assert_eq!(host_peer.recv(&mut buf).unwrap(), 4);
    assert_eq!(b.stats.hw_error_events, 1);
    assert!(logs[0].contains("Hardware Error"));
}

#[test]
fn controller_hangup_is_reported() {
    let (ctrl, ctrl_peer) = pair_stream();
    let (host, _host_peer) = pair_dgram();
    let mut b = Bridge::new(ctrl, host);
    drop(ctrl_peer);
    // a closed stream polls readable and reads 0 -> EOF error
    let r = b.step(Duration::from_millis(50), &mut |_| {});
    assert!(r.is_err() || r.as_ref().ok() == Some(&Step::Closed("controller hung up")));
}

#[test]
fn create_hci_reads_the_index() {
    let (vhci, peer) = pair_dgram();
    peer.send(&[0xff, 0x00, 0x02, 0x00]).unwrap();
    assert_eq!(create_hci(&vhci, Duration::from_secs(1)).unwrap(), 2);
    let mut req = [0u8; 4];
    assert_eq!(peer.recv(&mut req).unwrap(), 2);
    assert_eq!(&req[..2], &[0xff, 0x00]);
}

#[test]
fn create_hci_times_out() {
    let (vhci, _peer) = pair_dgram();
    let e = create_hci(&vhci, Duration::from_millis(100)).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn controller_command_waits_for_its_complete() {
    let (ctrl, mut peer) = pair_stream();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    // an unrelated event first, then the right Command Complete split in two writes
    peer.write_all(&[4, 0x0e, 4, 1, 0x03, 0x0c, 0, 4, 0x0e])
        .unwrap();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        peer.write_all(&[4, 1, 0x1a, 0xfc, 0]).unwrap();
        let mut got = [0u8; 16];
        let n = peer.read(&mut got).unwrap();
        got[..n].to_vec()
    });
    let cmd = h4::set_bdaddr_cmd([1, 2, 3, 4, 5, 6]);
    let (status, other) = controller_command(&ctrl, &cmd, Duration::from_secs(2)).unwrap();
    assert_eq!((status, other), (0, 1));
    assert_eq!(t.join().unwrap(), cmd);
}

#[test]
fn write_times_out_when_peer_never_reads() {
    let (a, _b) = pair_stream();
    let big = vec![0u8; 8 << 20];
    let e = write_all_timeout(&a, &big, Duration::from_millis(100)).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
}
