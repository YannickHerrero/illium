use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Global {
    pub theme: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wm {
    pub workspaces: u8,
    pub layout: String,
    pub gap: i32,
    pub outer_gap: i32,
    pub focus_follows_mouse: bool,
    #[serde(default = "enabled")]
    pub square_corners: bool,
    /// Logical width of the frame drawn around clients: accent when focused,
    /// overlay otherwise. 0 disables it.
    #[serde(default = "default_border")]
    pub border_width: i32,
}
fn enabled() -> bool {
    true
}
fn default_border() -> i32 {
    2
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    pub enabled: bool,
    pub position: String,
    pub height: i32,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    pub clock_format: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    pub width: i32,
    pub max_results: usize,
    pub show_descriptions: bool,
}
#[derive(Clone, Deserialize)]
pub struct Apps {
    pub apps: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize)]
pub struct Keys {
    pub keybindings: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub executable: Option<String>,
    pub class: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub ignore: bool,
    #[serde(default)]
    pub floating: bool,
    pub workspace: Option<u8>,
}
impl Rule {
    pub fn matches(&self, exe: &str, class: &str, title: &str) -> bool {
        [
            (&self.executable, exe),
            (&self.class, class),
            (&self.title, title),
        ]
        .iter()
        .all(|(pattern, text)| {
            pattern
                .as_ref()
                .is_none_or(|p| text.to_lowercase().contains(&p.to_lowercase()))
        })
    }
}
#[derive(Clone, Deserialize)]
pub struct Rules {
    pub rules: Vec<Rule>,
}
pub use winarchy_theme::Theme;
#[derive(Clone)]
pub struct Config {
    pub home: PathBuf,
    pub global: Global,
    pub wm: Wm,
    pub bar: Bar,
    pub launcher: Launcher,
    pub apps: Apps,
    pub keys: Keys,
    pub rules: Rules,
    pub theme: Theme,
}
/// Bar module names handled by the daemon itself; anything else is an applet.
pub const BUILTIN_MODULES: [&str; 7] = [
    "workspaces",
    "window-title",
    "volume",
    "battery",
    "clock",
    "cpu",
    "memory",
];
const DEFAULTS: &[(&str, &str)] = &[
    (
        "winarchy.toml",
        include_str!("../config/defaults/winarchy.toml"),
    ),
    ("wm.toml", include_str!("../config/defaults/wm.toml")),
    ("bar.toml", include_str!("../config/defaults/bar.toml")),
    (
        "launcher.toml",
        include_str!("../config/defaults/launcher.toml"),
    ),
    ("apps.toml", include_str!("../config/defaults/apps.toml")),
    (
        "keybindings.toml",
        include_str!("../config/defaults/keybindings.toml"),
    ),
    ("rules.toml", include_str!("../config/defaults/rules.toml")),
    (
        "themes/catppuccin-mocha.toml",
        include_str!("../config/themes/catppuccin-mocha.toml"),
    ),
    (
        "themes/catppuccin-latte.toml",
        include_str!("../config/themes/catppuccin-latte.toml"),
    ),
    (
        "applets/weather/applet.toml",
        include_str!("../config/applets/weather/applet.toml"),
    ),
    (
        "applets/weather/weather.ps1",
        include_str!("../config/applets/weather/weather.ps1"),
    ),
    (
        "applets/weather/view.slint",
        include_str!("../config/applets/weather/view.slint"),
    ),
    (
        "applets/weather/icon.svg",
        include_str!("../config/applets/weather/icon.svg"),
    ),
    (
        "applets/weather/sun.svg",
        include_str!("../config/applets/weather/sun.svg"),
    ),
    (
        "applets/weather/cloud.svg",
        include_str!("../config/applets/weather/cloud.svg"),
    ),
    (
        "applets/weather/rain.svg",
        include_str!("../config/applets/weather/rain.svg"),
    ),
    (
        "applets/weather/snow.svg",
        include_str!("../config/applets/weather/snow.svg"),
    ),
    (
        "applets/wifi/applet.toml",
        include_str!("../config/applets/wifi/applet.toml"),
    ),
    (
        "applets/wifi/wifi.ps1",
        include_str!("../config/applets/wifi/wifi.ps1"),
    ),
    (
        "applets/wifi/view.slint",
        include_str!("../config/applets/wifi/view.slint"),
    ),
    (
        "applets/wifi/icon.svg",
        include_str!("../config/applets/wifi/icon.svg"),
    ),
    (
        "applets/calendar/applet.toml",
        include_str!("../config/applets/calendar/applet.toml"),
    ),
    (
        "applets/calendar/view.slint",
        include_str!("../config/applets/calendar/view.slint"),
    ),
    (
        "applets/volume/applet.toml",
        include_str!("../config/applets/volume/applet.toml"),
    ),
    (
        "applets/volume/view.slint",
        include_str!("../config/applets/volume/view.slint"),
    ),
    (
        "applets/_template/applet.toml",
        include_str!("../config/applets/_template/applet.toml"),
    ),
    (
        "applets/_template/_template.ps1",
        include_str!("../config/applets/_template/_template.ps1"),
    ),
    (
        "applets/_template/view.slint",
        include_str!("../config/applets/_template/view.slint"),
    ),
    (
        "applets/_template/icon.svg",
        include_str!("../config/applets/_template/icon.svg"),
    ),
];
fn parse<T: serde::de::DeserializeOwned>(home: &Path, name: &str) -> Result<T, String> {
    let bytes = crate::files::read_config(&home.join(name))?;
    let s = std::str::from_utf8(&bytes).map_err(|e| format!("{name}: {e}"))?;
    toml::from_str(s).map_err(|e| format!("{name}: {e}"))
}
impl Config {
    pub fn home() -> PathBuf {
        winarchy_theme::config_home()
    }
    pub fn install(home: &Path) -> Result<(), String> {
        use std::io::Write;
        std::fs::create_dir_all(home.join("themes")).map_err(|e| e.to_string())?;
        for (name, content) in DEFAULTS {
            if let Some(parent) = home.join(name).parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(home.join(name))
            {
                Ok(mut f) => f.write_all(content.as_bytes()).map_err(|e| e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    pub fn load(home: &Path) -> Result<Self, String> {
        // Apply the same bounds at startup/reload as in the directory watcher.
        crate::files::snapshot(home)?;
        let global: Global = parse(home, "winarchy.toml")?;
        if !winarchy_theme::valid_name(&global.theme) {
            return Err("invalid theme name".into());
        }
        let c = Self {
            home: home.into(),
            theme: parse(home, &format!("themes/{}.toml", global.theme))?,
            global,
            wm: parse(home, "wm.toml")?,
            bar: parse(home, "bar.toml")?,
            launcher: parse(home, "launcher.toml")?,
            apps: parse(home, "apps.toml")?,
            keys: parse(home, "keybindings.toml")?,
            rules: parse(home, "rules.toml")?,
        };
        if c.wm.workspaces != 9
            || c.wm.layout != "fibonacci"
            || !(0..=100).contains(&c.wm.gap)
            || !(0..=100).contains(&c.wm.outer_gap)
        {
            return Err("wm: require 9 workspaces, fibonacci, gaps 0..100".into());
        }
        if !(16..=100).contains(&c.bar.height)
            || !["top", "bottom"].contains(&c.bar.position.as_str())
        {
            return Err("bar: invalid height or position".into());
        }
        if !(200..=2000).contains(&c.launcher.width) || !(1..=30).contains(&c.launcher.max_results)
        {
            return Err("launcher: invalid dimensions".into());
        }
        crate::keyboard::parse(&c.keys)?;
        for (index, modules) in [&c.bar.left, &c.bar.center, &c.bar.right]
            .iter()
            .enumerate()
        {
            for module in *modules {
                if module == "workspaces" && index != 0 {
                    return Err(format!("bar: unsupported module or placement: {module}"));
                }
                if !BUILTIN_MODULES.contains(&module.as_str())
                    && !crate::applets::exists(home, module)
                {
                    return Err(format!(
                        "bar: unknown module {module}: no built-in module and no applets/{module}/applet.toml"
                    ));
                }
            }
        }
        crate::clock::validate(&c.bar.clock_format)?;
        c.theme.validate()?;
        for r in &c.rules.rules {
            if r.workspace.is_some_and(|n| !(1..=9).contains(&n)) {
                return Err("rule workspace must be 1..9".into());
            }
        }
        Ok(c)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_invalid_reload() {
        let p = std::env::temp_dir().join(format!("winarchy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Config::install(&p).unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.global.theme, "catppuccin-mocha");
        assert_eq!(c.keys.keybindings.len(), 47);
        std::fs::write(p.join("wm.toml"), "invalid").unwrap();
        assert!(Config::load(&p).is_err());
        Config::install(&p).unwrap();
        assert!(Config::load(&p).is_err());
        assert_eq!(c.wm.gap, 16);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn rules() {
        let r = Rule {
            executable: Some("NOTEPAD".into()),
            title: Some("notes".into()),
            ..Rule::default()
        };
        assert!(r.matches("notepad.exe", "", "My notes"));
        assert!(!r.matches("notepad.exe", "", "Other"));
    }
    #[test]
    fn palettes() {
        for (_, s) in DEFAULTS.iter().filter(|(n, _)| n.starts_with("themes/")) {
            assert!(Theme::parse(s).is_ok());
        }
    }
}
