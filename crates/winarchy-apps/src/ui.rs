//! Generated Slint components and the theme glue every application uses.
slint::include_modules!();
use winarchy_theme::Theme;
pub fn color(hex: &str) -> slint::Color {
    let (r, g, b) = winarchy_theme::rgb(hex).unwrap_or((255, 0, 255));
    slint::Color::from_rgb_u8(r, g, b)
}
/// Pushes the theme into the window's `Palette` global.
pub fn apply(palette: Palette<'_>, theme: &Theme) {
    palette.set_background_opacity(theme.background_opacity);
    palette.set_bg(color(&theme.background));
    palette.set_surface(color(&theme.surface));
    palette.set_overlay(color(&theme.overlay));
    palette.set_fg(color(&theme.text));
    palette.set_muted(color(&theme.subtext));
    palette.set_accent(color(&theme.accent));
    palette.set_green(color(&theme.green));
    palette.set_yellow(color(&theme.yellow));
    palette.set_red(color(&theme.red));
}
/// Subscribe without IO on the UI thread; hidden resident windows update too.
pub fn watch_theme<T>(window: &T) -> Result<winarchy_theme::live::Subscription, String>
where
    T: slint::ComponentHandle + 'static,
    for<'a> Palette<'a>: slint::Global<'a, T>,
{
    let weak = window.as_weak();
    winarchy_theme::live::watch(config_home(), move |theme| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                apply(window.global::<Palette>(), &theme);
                window.window().request_redraw();
            }
        });
    })
}
/// The configuration home the daemon uses, for the theme.
pub fn config_home() -> std::path::PathBuf {
    std::env::var_os("WINARCHY_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                .join(".config/winarchy")
        })
}
pub fn strings(lines: &[&str]) -> slint::ModelRc<slint::SharedString> {
    slint::ModelRc::new(slint::VecModel::from(
        lines
            .iter()
            .map(|l| slint::SharedString::from(*l))
            .collect::<Vec<_>>(),
    ))
}
pub fn init_com() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
}
/// Brings an already shown window to the front, for a second `show` request.
pub fn raise(window: &impl slint::ComponentHandle) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
    if let Ok(handle) = window.window().window_handle().window_handle()
        && let RawWindowHandle::Win32(h) = handle.as_raw()
    {
        unsafe {
            let _ = SetForegroundWindow(HWND(h.hwnd.get() as *mut _));
        }
    }
}
