use std::sync::atomic::AtomicBool;
use walpad_hw::Root;
use walpad_shell::shell::{Options, Shell};

/// Must equal the capability walpad-runtime requires for WALPAD_SHELL (tests/capability.rs).
const CAPABILITIES: &str = "shell";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => return println!("walpad-shell {}", env!("CARGO_PKG_VERSION")),
        // walpad-runtime honours WALPAD_SHELL=rust only if this list has `shell`.  The default
        // runtime.conf keeps WALPAD_SHELL=python, so advertising it switches nothing by itself.
        Some("--capabilities") => return println!("{CAPABILITIES}"),
        _ => {}
    }
    // WALPAD_X_SOCKET overrides <root>/tmp/.X11-unix/X0 (host sessions use :5)
    let x_socket = std::env::var_os("WALPAD_X_SOCKET").map(std::path::PathBuf::from);
    let opts = Options {
        root: Root::from_env(),
        x_socket,
        core_socket: None,
        fonts_dir: None,
    };
    let mut shell = match Shell::new(opts) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("walpad-shell: {e}");
            std::process::exit(1);
        }
    };
    let stop = AtomicBool::new(false);
    if let Err(e) = shell.run(&stop) {
        eprintln!("walpad-shell: {e}");
        std::process::exit(1);
    }
}
