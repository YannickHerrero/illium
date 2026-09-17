#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;
#[cfg(windows)]
mod picker;

fn main() {
    #[cfg(windows)]
    if let Err(error) = native::run() {
        eprintln!("winarchy-browser: {error}");
        native::show_error(&error.to_string());
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchy-browser requires Windows and the WebView2 Evergreen runtime");
        std::process::exit(1);
    }
}
