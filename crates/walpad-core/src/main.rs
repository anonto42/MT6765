use std::sync::atomic::AtomicBool;
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;

const USAGE: &str = "walpad-core [--version] [--fake] [--root DIR]\n  WALPAD_ROOT sets the path prefix (default /); --fake fills it with sample hardware values.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("walpad-core {}", env!("CARGO_PKG_VERSION"));
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
    if args.iter().any(|a| a == "--fake") {
        if let Err(e) = fake::populate(root.base()) {
            eprintln!("walpad-core: cannot populate fake root: {e}");
            std::process::exit(1);
        }
    }
    let mut d = match Daemon::new(root) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("walpad-core: startup failed: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("walpad-core: listening on {}", d.socket_path().display());
    let stop = AtomicBool::new(false);
    d.run(&stop);
}
