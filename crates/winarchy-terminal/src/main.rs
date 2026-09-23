#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod blur;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod resident;
#[cfg(windows)]
mod wake;
#[cfg(windows)]
fn log(message: &str) {
    use std::io::Write;
    let home = winarchy_theme::config_home();
    // Bounded diagnostics only, never terminal output or typed input.
    let path = home.join("terminal.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let backup = home.join("terminal.log.1");
        let _ = std::fs::remove_file(&backup);
        // A competing launcher may be writing/rotating too. Never grow the
        // original without bound when Windows refuses a rename.
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
            let text: Vec<u16> = e.encode_utf16().chain([0]).collect();
            if std::env::args().len() == 1 {
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        None,
                        windows::core::PCWSTR(text.as_ptr()),
                        windows::core::w!("Winarchy Terminal"),
                        windows::Win32::UI::WindowsAndMessaging::MB_OK
                            | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                    );
                }
            }
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchy-terminal requires Windows");
        std::process::exit(1);
    }
}
