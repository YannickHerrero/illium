//! One executable, one subcommand per application: `winarchy-apps <name>`.
//! Kept out of the daemon so the process holding the keyboard hook contains
//! no screen-capture, file or process-management code, and so an application
//! crash never takes the window manager down.
#![cfg_attr(windows, windows_subsystem = "windows")]
// Only the pure models and their tests build elsewhere than Windows.
#![cfg_attr(not(windows), allow(dead_code))]
mod files;
mod key;
mod log;
#[cfg(windows)]
mod shot;
mod tasks;
#[cfg(windows)]
mod ui;
/// Exit code for a missing or unknown subcommand, as for `winarchyctl` syntax errors.
const USAGE: i32 = 2;
const UNKNOWN: &str = "unknown application (expected: shot, tasks, files)";
fn main() {
    std::panic::set_hook(Box::new(|info| log::write(&format!("panic: {info}"))));
    let name = std::env::args().nth(1).unwrap_or_default();
    if let Err(e) = run(&name) {
        log::write(&format!("{name}: {e}"));
        std::process::exit(if e == UNKNOWN { USAGE } else { 1 });
    }
}
#[cfg(windows)]
fn run(name: &str) -> Result<(), String> {
    match name {
        "shot" => shot::run(),
        "tasks" => tasks::run(),
        "files" => files::run(),
        _ => Err(UNKNOWN.into()),
    }
}
#[cfg(not(windows))]
fn run(_name: &str) -> Result<(), String> {
    Err("winarchy-apps requires Windows".into())
}
