use std::sync::atomic::AtomicBool;
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;

const USAGE: &str = "walpad-core [--version] [--capabilities] [--fake] [--shadow] [--device] [--input] [--root DIR]\n  WALPAD_ROOT sets the path prefix (default /); --fake fills it with sample hardware values.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("walpad-core {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--capabilities") {
        // Subsystems this build can run live (walpad-runtime only honours `rust` for these).
        println!("ipc shadow device input");
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return;
    }
    let mut root = Root::from_env();
    if let Some(i) = args.iter().position(|a| a == "--root") {
        match args.get(i + 1) {
            Some(d) => root = Root::new(d),
            None => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let mut _fake_supplicant = None;
    if args.iter().any(|a| a == "--fake") {
        if let Err(e) = fake::populate(root.base()) {
            eprintln!("walpad-core: cannot populate fake root: {e}");
            std::process::exit(1);
        }
        let conf = root.p("/etc/wpa_supplicant/wpa_supplicant.conf");
        let _ = std::fs::create_dir_all(root.p("/etc/wpa_supplicant"));
        match walpad_core::wpa::fake::spawn(
            root.base(),
            walpad_core::wpa::fake::default_aps(),
            Some(conf),
        ) {
            Ok(s) => _fake_supplicant = Some(s),
            Err(e) => eprintln!("walpad-core: fake supplicant unavailable: {e}"),
        }
    }
    let conf_path = root.p("/etc/walpad/runtime.conf");
    let mut d = match Daemon::new(root) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("walpad-core: startup failed: {e}");
            std::process::exit(1);
        }
    };
    let conf = std::fs::read_to_string(conf_path).unwrap_or_default();
    if args.iter().any(|a| a == "--shadow") || conf.lines().any(|l| l.trim() == "WALPAD_SHADOW=1") {
        d.enable_shadow();
        eprintln!("walpad-core: SHADOW mode: logging WOULD lines, no hardware writes by policy");
    }
    let (dev, inp) = (
        args.iter().any(|a| a == "--device"),
        args.iter().any(|a| a == "--input"),
    );
    if dev || inp {
        d.enable_live(dev, inp);
        eprintln!("walpad-core: LIVE subsystems: device={dev} input={inp}");
    }
    eprintln!("walpad-core: listening on {}", d.socket_path().display());
    let stop = AtomicBool::new(false);
    d.run(&stop);
}
