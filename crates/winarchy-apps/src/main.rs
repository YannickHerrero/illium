//! One executable, one subcommand per application: `winarchy-apps <name>`.
//! Kept out of the daemon so the process holding the keyboard hook contains
//! no screen-capture, file or process-management code, and so an application
//! crash never takes the window manager down.
#![cfg_attr(windows, windows_subsystem = "windows")]
mod log;
#[cfg(windows)]
mod shot;
/// Exit code for a missing or unknown subcommand, as for `winarchyctl` syntax errors.
const USAGE: i32 = 2;
fn main() {
    std::panic::set_hook(Box::new(|info| log::write(&format!("panic: {info}"))));
    let name = std::env::args().nth(1).unwrap_or_default();
    let result = run(&name);
    if let Err(e) = result {
        log::write(&format!("{name}: {e}"));
        std::process::exit(if e == UNKNOWN { USAGE } else { 1 });
    }
}
const UNKNOWN: &str = "unknown application (expected: shot)";
#[cfg(windows)]
fn run(name: &str) -> Result<(), String> {
    match name {
        "shot" => shot::run(),
        _ => Err(UNKNOWN.into()),
    }
}
#[cfg(not(windows))]
fn run(_name: &str) -> Result<(), String> {
    Err("winarchy-apps requires Windows".into())
}
