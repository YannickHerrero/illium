//! Generated Slint components and the theme glue every application uses.
slint::include_modules!();
use winarchy_theme::Theme;
pub fn color(hex: &str) -> slint::Color {
    let (r, g, b) = winarchy_theme::rgb(hex).unwrap_or((255, 0, 255));
    slint::Color::from_rgb_u8(r, g, b)
}
/// Pushes the theme into the window's `Palette` global.
pub fn apply(palette: Palette<'_>, theme: &Theme) {
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
