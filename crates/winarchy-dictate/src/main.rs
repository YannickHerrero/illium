#![cfg_attr(windows, windows_subsystem = "windows")]
//! Hold-to-talk dictation for Winarchy: the daemon relays the bound key going
//! down and up over a pipe; this resident records the default microphone,
//! transcribes locally with Parakeet and pastes the text where the caret is.
#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod indicator;
#[cfg(windows)]
mod model;
#[cfg(windows)]
mod paste;
#[cfg(windows)]
mod resident;
/// Bounded diagnostics only, never audio or transcribed text.
pub fn log(message: &str) {
    use std::io::Write;
    let home = winarchy_theme::config_home();
    let path = home.join("dictate.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let backup = home.join("dictate.log.1");
        let _ = std::fs::remove_file(&backup);
        if std::fs::rename(&path, backup).is_err() {
            return;
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "pid={} {message}", std::process::id());
    }
}
fn main() {
    #[cfg(windows)]
    {
        std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
        if let Err(e) = resident::run() {
            log(&e);
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchy-dictate requires Windows");
        std::process::exit(1);
    }
}
