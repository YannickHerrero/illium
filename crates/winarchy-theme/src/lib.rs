//! Theme palette read from `themes/<name>.toml` under the configuration home.
//! No graphics dependency: the daemon and every application convert the
//! `#rrggbb` strings to their own color type.
#[cfg(windows)]
pub mod blur;
pub mod dynamic;
#[cfg(feature = "live")]
pub mod live;
pub mod opacity;
#[cfg(feature = "assets")]
pub mod pack;
#[cfg(feature = "assets")]
pub mod preview;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub name: String,
    pub background: String,
    pub surface: String,
    pub overlay: String,
    pub text: String,
    pub subtext: String,
    pub accent: String,
    pub green: String,
    pub yellow: String,
    pub red: String,
    /// Optional terminal colors, in ANSI order (black through white).
    #[serde(default)]
    pub ansi: Option<Vec<String>>,
    /// Optional bright terminal colors, in the same order.
    #[serde(default)]
    pub brights: Option<Vec<String>>,
    /// Background opacity shared by Winarchy applications (web pages stay opaque).
    #[serde(
        default = "default_background_opacity",
        alias = "terminal_background_opacity"
    )]
    pub background_opacity: f32,
    /// Windows color mode to apply with this theme: "dark" or "light".
    #[serde(default)]
    pub mode: Option<String>,
    /// Blur behind translucent application backgrounds. A `winarchy.toml`
    /// preference, not a theme key: it survives theme switches.
    #[serde(skip)]
    pub background_blur: bool,
}
fn default_background_opacity() -> f32 {
    0.85
}
#[derive(Deserialize)]
struct Global {
    theme: String,
    #[serde(default)]
    background_blur: bool,
}
/// Configuration location shared by the daemon, installer and external consumers.
pub fn config_home() -> std::path::PathBuf {
    std::env::var_os("WINARCHY_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap_or_default(),
            )
            .join(".config/winarchy")
        })
}
pub const DEFAULT_NAME: &str = "catppuccin-mocha";
const DEFAULT: &str = include_str!("../../../config/themes/catppuccin-mocha.toml");
/// Files above this size are not theme files.
const MAX_BYTES: u64 = 64 * 1024;
/// Rejects unsafe names: a theme is a plain file stem under `themes/`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', '.'])
}
/// Red, green and blue components of a `#rrggbb` color.
pub fn rgb(color: &str) -> Option<(u8, u8, u8)> {
    let hex = color.strip_prefix('#').filter(|h| h.len() == 6)?;
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}
fn read(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("{}: file too large", path.display()));
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
impl Theme {
    pub fn parse(text: &str) -> Result<Self, String> {
        let theme: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        theme.validate()?;
        Ok(theme)
    }
    /// The embedded Catppuccin Mocha palette.
    pub fn default_theme() -> Self {
        Self::parse(DEFAULT).expect("embedded theme is valid")
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.background_opacity.is_finite() || !(0.0..=1.0).contains(&self.background_opacity) {
            return Err("background_opacity must be finite and between 0 and 1".into());
        }
        if let Some(mode) = &self.mode
            && !["dark", "light"].contains(&mode.as_str())
        {
            return Err("theme mode must be \"dark\" or \"light\"".into());
        }
        for (name, palette) in [("ansi", &self.ansi), ("brights", &self.brights)] {
            if palette.as_ref().is_some_and(|colors| colors.len() != 8) {
                return Err(format!("theme {name} must contain exactly eight colors"));
            }
        }
        for color in self
            .colors()
            .into_iter()
            .chain(self.ansi.iter().flatten().map(String::as_str))
            .chain(self.brights.iter().flatten().map(String::as_str))
        {
            if rgb(color).is_none() {
                return Err(format!("invalid theme color: {color}"));
            }
        }
        Ok(())
    }
    pub fn colors(&self) -> [&str; 9] {
        [
            &self.background,
            &self.surface,
            &self.overlay,
            &self.text,
            &self.subtext,
            &self.accent,
            &self.green,
            &self.yellow,
            &self.red,
        ]
    }
    fn global(home: &Path) -> Result<Global, String> {
        let global: Global = toml::from_str(&read(&home.join("winarchy.toml"))?)
            .map_err(|e| format!("winarchy.toml: {e}"))?;
        if !valid_name(&global.theme) {
            return Err("invalid theme name".into());
        }
        Ok(global)
    }
    /// Name of the theme selected in `winarchy.toml`.
    pub fn selected(home: &Path) -> Result<String, String> {
        Ok(Self::global(home)?.theme)
    }
    /// The theme `name` from `themes/` under `home`.
    pub fn load(home: &Path, name: &str) -> Result<Self, String> {
        if !valid_name(name) {
            return Err("invalid theme name".into());
        }
        let path = home.join("themes").join(format!("{name}.toml"));
        Self::parse(&read(&path)?).map_err(|e| format!("themes/{name}.toml: {e}"))
    }
    /// Selected theme with the daemon's temporary application opacity override.
    pub fn effective(home: &Path) -> Result<Self, String> {
        let Global {
            theme: name,
            background_blur,
        } = Self::global(home)?;
        let mut theme = Self::load(home, &name)?;
        dynamic::apply(home, &name, &mut theme)?;
        opacity::apply(home, &name, &mut theme);
        theme.background_blur = background_blur;
        Ok(theme)
    }
    /// The selected theme, or the embedded default when the configuration is
    /// missing or broken: an application must still open with a readable palette.
    pub fn current(home: &Path) -> Self {
        Self::effective(home).unwrap_or_else(|_| Self::default_theme())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn home() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "winarchy-theme-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("themes")).unwrap();
        p
    }
    #[test]
    fn default_palette() {
        let t = Theme::default_theme();
        assert_eq!(t.name, "Catppuccin Mocha");
        assert_eq!(rgb(&t.background), Some((0x1e, 0x1e, 0x2e)));
        assert_eq!(t.mode.as_deref(), Some("dark"));
    }
    #[test]
    fn rejects_bad_colors_and_modes() {
        assert!(Theme::parse(&DEFAULT.replace("#1e1e2e", "1e1e2e")).is_err());
        assert!(Theme::parse(&DEFAULT.replace("#1e1e2e", "#1e1e2g")).is_err());
        assert!(Theme::parse(&DEFAULT.replace("\"dark\"", "\"blue\"")).is_err());
        assert!(Theme::parse(&format!("{DEFAULT}extra = 1\n")).is_err());
        assert_eq!(rgb("#zz0000"), None);
        assert_eq!(rgb("#fff"), None);
    }
    #[test]
    fn background_opacity() {
        assert_eq!(Theme::parse(DEFAULT).unwrap().background_opacity, 0.85);
        for key in ["background_opacity", "terminal_background_opacity"] {
            for value in ["0.0", "0.85", "1.0"] {
                let t = Theme::parse(&format!("{DEFAULT}\n{key} = {value}\n")).unwrap();
                assert_eq!(t.background_opacity, value.parse::<f32>().unwrap());
            }
            for value in ["-0.1", "1.1", "nan", "inf", "-inf", "\"0.85\""] {
                assert!(Theme::parse(&format!("{DEFAULT}\n{key} = {value}\n")).is_err());
            }
        }
        assert!(
            Theme::parse(&format!(
                "{DEFAULT}\nbackground_opacity = 0.5\nterminal_background_opacity = 0.8\n"
            ))
            .is_err()
        );
    }
    #[test]
    fn terminal_palettes() {
        for source in [
            DEFAULT,
            include_str!("../../../config/themes/catppuccin-latte.toml"),
        ] {
            let theme = Theme::parse(source).unwrap();
            assert_eq!(theme.ansi.as_ref().unwrap()[1], theme.red);
            assert_eq!(theme.brights.as_ref().unwrap()[2], theme.green);
            let legacy = source.split("# Terminal palette").next().unwrap();
            let theme = Theme::parse(legacy).unwrap();
            assert!(theme.ansi.is_none());
            assert!(theme.brights.is_none());
            for field in ["ansi", "brights"] {
                for count in [0, 7, 9] {
                    let colors = vec!["\"#123456\""; count].join(", ");
                    assert!(
                        Theme::parse(&format!("{legacy}{field} = [{colors}]\n")).is_err(),
                        "{field} with {count} colors must be rejected"
                    );
                }
                let colors = ["\"#123456\""; 8].join(", ");
                let valid = format!("{legacy}{field} = [{colors}]\n");
                assert!(Theme::parse(&valid).is_ok());
                assert!(Theme::parse(&valid.replacen("#123456", "#xyzxyz", 1)).is_err());
            }
        }
    }
    #[test]
    fn selected_and_loaded_from_home() {
        let home = home();
        std::fs::write(home.join("winarchy.toml"), "theme = \"mine\"\n").unwrap();
        std::fs::write(
            home.join("themes/mine.toml"),
            DEFAULT.replace("Catppuccin Mocha", "Mine"),
        )
        .unwrap();
        assert_eq!(Theme::selected(&home).unwrap(), "mine");
        assert_eq!(Theme::current(&home).name, "Mine");
        assert!(!Theme::current(&home).background_blur);
        std::fs::write(
            home.join("winarchy.toml"),
            "theme = \"mine\"\nbackground_blur = true\n",
        )
        .unwrap();
        assert!(Theme::current(&home).background_blur);
        assert!(Theme::parse(&format!("{DEFAULT}background_blur = true\n")).is_err());
        std::fs::write(home.join("winarchy.toml"), "theme = \"../x\"\n").unwrap();
        assert!(Theme::selected(&home).is_err());
        assert!(Theme::load(&home, "missing").is_err());
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn falls_back_without_configuration() {
        let home = home();
        assert_eq!(Theme::current(&home), Theme::default_theme());
        std::fs::write(home.join("winarchy.toml"), "theme = \"missing\"\n").unwrap();
        assert_eq!(Theme::current(&home), Theme::default_theme());
        std::fs::remove_dir_all(home).unwrap();
    }
}
