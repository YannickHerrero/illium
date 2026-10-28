#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod browser_view;
#[cfg(windows)]
mod input_mirror;
#[cfg(windows)]
mod leader_panel;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod picker;
#[cfg(windows)]
mod resident;
#[cfg(windows)]
mod surface;

fn main() {
    // A panic inside a window procedure or COM callback aborts the process
    // without any console; keep its message next to the resident log.
    #[cfg(windows)]
    std::panic::set_hook(Box::new(|info| resident::log(&format!("panic: {info}"))));
    #[cfg(windows)]
    if let Err(error) = illium_theme::fonts::install() {
        resident::log(&format!("fonts: {error}"));
    }
    #[cfg(windows)]
    if let Err(error) = resident::run() {
        resident::log(&format!("illium-browser: {error}"));
        if !std::env::args().any(|a| matches!(a.as_str(), "--serve" | "--status" | "--quit")) {
            native::show_error(&error);
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("illium-browser requires Windows and the WebView2 Evergreen runtime");
        std::process::exit(1);
    }
}
