use walpad_hw::Root;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("walpad-ctl {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.first().map(String::as_str) == Some("key-held") {
        std::process::exit(walpad_ctl::key_held_status(&args, &Root::from_env()));
    }
    match walpad_ctl::run(&args, &Root::from_env()) {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
