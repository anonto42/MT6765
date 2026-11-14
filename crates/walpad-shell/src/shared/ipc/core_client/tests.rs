use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;

struct Core {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Core {
    fn start(root: &std::path::Path) -> Core {
        let mut d = Daemon::new(Root::new(root)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        Core {
            stop,
            h: Some(std::thread::spawn(move || d.run(&s2))),
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

/// Events not yet consumed by a `wait` (several can arrive in one tick).
#[derive(Default)]
struct Log(std::collections::VecDeque<CoreEvent>);

fn wait(
    c: &mut CoreClient,
    log: &mut Log,
    what: &str,
    mut pred: impl FnMut(&CoreEvent) -> bool,
) -> CoreEvent {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(6) {
        while let Some(e) = log.0.pop_front() {
            if pred(&e) {
                return e;
            }
        }
        log.0.extend(c.tick(Instant::now()));
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn connects_gets_status_sends_commands_and_reconnects() {
    let root = std::env::temp_dir().join(format!("walpad-client-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    fake::populate(&root).unwrap();
    let sock = root.join("run/walpad/control.sock");

    let mut c = CoreClient::new(sock.clone());
    // nothing there yet: no events, not connected, no panic
    assert!(c.tick(Instant::now()).is_empty());
    assert!(!c.connected());

    let mut core = Core::start(&root);
    // (the retry timer is 2 s: use a fresh client so the test does not wait)
    let mut c = CoreClient::new(sock.clone());
    let mut log = Log::default();
    wait(&mut c, &mut log, "Connected", |e| {
        *e == CoreEvent::Connected
    });
    let CoreEvent::Status(s) = wait(&mut c, &mut log, "first status", |e| {
        matches!(e, CoreEvent::Status(_))
    }) else {
        unreachable!()
    };
    assert_eq!(s.brightness, 120);
    assert_eq!(s.battery.percent, Some(66));

    // a command changes state and the status event follows
    c.send("set_brightness", json!({"value": 200}));
    wait(
        &mut c,
        &mut log,
        "brightness 200",
        |e| matches!(e, CoreEvent::Status(s) if s.brightness == 200),
    );
    // a rejected command comes back as a Reply with the protocol error
    let id = c.send("set_brightness", json!({"value": 999})).unwrap();
    let CoreEvent::Reply {
        error: Some(e),
        ok: false,
        ..
    } = wait(
        &mut c,
        &mut log,
        "error reply",
        |ev| matches!(ev, CoreEvent::Reply { id: rid, .. } if *rid == id),
    )
    else {
        unreachable!()
    };
    assert_eq!(e.code, "invalid_args");
    assert!(e.msg.contains("0-255"), "{}", e.msg);
    // a successful request returns its data
    let id = c.send("get_status", Value::Null).unwrap();
    let CoreEvent::Reply { ok: true, data, .. } = wait(
        &mut c,
        &mut log,
        "status reply",
        |ev| matches!(ev, CoreEvent::Reply { id: rid, .. } if *rid == id),
    ) else {
        unreachable!()
    };
    assert_eq!(data["brightness"], 200);

    // the core goes away: Disconnected, then it reconnects when the core is back
    core.stop();
    wait(&mut c, &mut log, "Disconnected", |e| {
        *e == CoreEvent::Disconnected
    });
    assert!(!c.connected());
    let _core2 = Core::start(&root);
    wait(&mut c, &mut log, "reconnect", |e| {
        *e == CoreEvent::Connected
    });
    wait(&mut c, &mut log, "status after reconnect", |e| {
        matches!(e, CoreEvent::Status(_))
    });
    assert!(c.connected());
}
