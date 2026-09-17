#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;
#[cfg(windows)]
mod picker;
#[cfg(windows)]
mod resident;

fn main() {
    #[cfg(windows)]
    if let Err(error) = resident::run() {
        resident::log(&format!("winarchy-browser: {error}"));
        if !std::env::args().any(|a| matches!(a.as_str(), "--serve" | "--status" | "--quit")) {
            native::show_error(&error);
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchy-browser requires Windows and the WebView2 Evergreen runtime");
        std::process::exit(1);
    }
}
