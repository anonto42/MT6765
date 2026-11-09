fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        // The `shell` capability is added by M5; until then walpad-runtime keeps the Tk launcher.
        Some("--capabilities") => println!(),
        _ => {}
    }
}
