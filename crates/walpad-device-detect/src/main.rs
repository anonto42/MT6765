//! walpad-device-detect: System inspection and device profile detection tool.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod probe;
pub mod report;

use std::path::{Path, PathBuf};
use walpad_hw::domain::device_profile::DeviceProfile;
use walpad_hw::Root;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Opts {
    json: bool,
    root: Root,
    profile_path: Option<PathBuf>,
}

fn parse_args() -> Result<Opts, i32> {
    let mut args = std::env::args().skip(1);
    let mut json = false;
    let mut root = Root::from_env();
    let mut profile_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" | "-j" => json = true,
            "--version" | "-v" => {
                println!("walpad-device-detect {VERSION}");
                return Err(0);
            }
            "--help" | "-h" => {
                print_help();
                return Err(0);
            }
            "--root" | "-r" => {
                if let Some(r) = args.next() {
                    root = Root::new(r);
                } else {
                    eprintln!("Error: --root requires a path argument");
                    return Err(1);
                }
            }
            "--profile" | "-p" => {
                if let Some(p) = args.next() {
                    profile_path = Some(PathBuf::from(p));
                } else {
                    eprintln!("Error: --profile requires a path argument");
                    return Err(1);
                }
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_help();
                return Err(1);
            }
        }
    }

    Ok(Opts {
        json,
        root,
        profile_path,
    })
}

fn print_help() {
    println!(
        "Usage: walpad-device-detect [OPTIONS]\n\n\
         Options:\n  \
           --json, -j           Output diagnostic report as machine-readable JSON\n  \
           --profile, -p <PATH> Evaluate system against a specific device.toml profile\n  \
           --root, -r <PATH>    Alternative sysfs/proc root path (default: $WALPAD_ROOT or /)\n  \
           --version, -v        Print version information\n  \
           --help, -h           Print this help message"
    );
}

fn main() {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(code) => std::process::exit(code),
    };

    let audit = probe::probe_all(&opts.root);

    let profile = if let Some(ref path) = opts.profile_path {
        match DeviceProfile::load_from_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "Warning: failed to load profile from {}: {e}",
                    path.display()
                );
                DeviceProfile::detect(&opts.root)
            }
        }
    } else {
        // Look in /etc/walpad/device.toml or platforms/ directory or detect
        let etc_path = opts.root.p("/etc/walpad/device.toml");
        let default_platform = Path::new("platforms/walton/walpad-8g/device.toml");
        if etc_path.exists() {
            DeviceProfile::load_from_file(&etc_path)
                .unwrap_or_else(|_| DeviceProfile::detect(&opts.root))
        } else if default_platform.exists()
            && audit
                .soc_model
                .as_deref()
                .is_some_and(|s| s.contains("MT6765"))
        {
            DeviceProfile::load_from_file(default_platform)
                .unwrap_or_else(|_| DeviceProfile::detect(&opts.root))
        } else {
            DeviceProfile::detect(&opts.root)
        }
    };

    let rep = report::build_report(audit, profile);

    if opts.json {
        match report::format_json(&rep) {
            Ok(j) => println!("{j}"),
            Err(e) => {
                eprintln!("Error formatting JSON: {e}");
                std::process::exit(1);
            }
        }
    } else {
        print!("{}", report::format_human_report(&rep));
    }
}
