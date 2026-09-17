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
pub const BUILTIN_MODULES: [&str; 8] = [
    "workspaces",
    "window-title",
    "volume",
    "battery",
    "clock",
    "cpu",
    "memory",
    "separator",
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
        "terminal.toml",
        include_str!("../config/defaults/terminal.toml"),
    ),
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
        "applets/wifi/lock.svg",
        include_str!("../config/applets/wifi/lock.svg"),
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
        "applets/volume/speaker.svg",
        include_str!("../config/applets/volume/speaker.svg"),
    ),
    (
        "applets/volume/speaker-muted.svg",
        include_str!("../config/applets/volume/speaker-muted.svg"),
    ),
    (
        "applets/volume/output.svg",
        include_str!("../config/applets/volume/output.svg"),
    ),
    (
        "applets/volume/input.svg",
        include_str!("../config/applets/volume/input.svg"),
    ),
    (
        "applets/volume/source.svg",
        include_str!("../config/applets/volume/source.svg"),
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
// Bundled data, not runtime downloads. Keep existing installed files intact.
const DEFAULT_ASSETS: &[(&str, &[u8])] = &[
    (
        "themes/catppuccin-mocha/preview.png",
        include_bytes!("../config/themes/catppuccin-mocha/preview.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/1-totoro.png",
        include_bytes!("../config/themes/catppuccin-mocha/wallpapers/1-totoro.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/2-waves.png",
        include_bytes!("../config/themes/catppuccin-mocha/wallpapers/2-waves.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/3-blue-eye.png",
        include_bytes!("../config/themes/catppuccin-mocha/wallpapers/3-blue-eye.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/omarchy.png",
        include_bytes!("../config/themes/catppuccin-mocha/wallpapers/omarchy.png"),
    ),
    (
        "themes/catppuccin-mocha/SOURCES.md",
        include_bytes!("../config/themes/catppuccin-mocha/SOURCES.md"),
    ),
    (
        "themes/catppuccin-mocha/LICENSE",
        include_bytes!("../config/themes/catppuccin-mocha/LICENSE"),
    ),
    (
        "themes/catppuccin-latte/preview.png",
        include_bytes!("../config/themes/catppuccin-latte/preview.png"),
    ),
    (
        "themes/catppuccin-latte/wallpapers/1-color-fade.png",
        include_bytes!("../config/themes/catppuccin-latte/wallpapers/1-color-fade.png"),
    ),
    (
        "themes/catppuccin-latte/wallpapers/omarchy.png",
        include_bytes!("../config/themes/catppuccin-latte/wallpapers/omarchy.png"),
    ),
    (
        "themes/catppuccin-latte/SOURCES.md",
        include_bytes!("../config/themes/catppuccin-latte/SOURCES.md"),
    ),
    (
        "themes/catppuccin-latte/LICENSE",
        include_bytes!("../config/themes/catppuccin-latte/LICENSE"),
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
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        for (name, bytes) in DEFAULTS
            .iter()
            .map(|(name, text)| (*name, text.as_bytes()))
            .chain(DEFAULT_ASSETS.iter().copied())
        {
            let mut directory = home.to_path_buf();
            for component in Path::new(name).parent().unwrap().components() {
                directory.push(component);
                crate::files::create_directory(&directory)?;
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(home.join(name))
            {
                Ok(mut f) => f.write_all(bytes).map_err(|e| e.to_string())?,
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
        assert_eq!(c.keys.keybindings.len(), 53);
        assert_eq!(c.keys.keybindings["Alt+B"], "spawn browser");
        assert_eq!(c.apps.apps["browser"], "winarchy-browser.exe");
        for file in [
            "applet.toml",
            "wifi.ps1",
            "view.slint",
            "icon.svg",
            "lock.svg",
        ] {
            assert!(
                p.join("applets/wifi").join(file).is_file(),
                "missing Wi-Fi asset: {file}"
            );
        }
        assert!(
            crate::applets::load(&p, "wifi")
                .unwrap()
                .manifest
                .wifi_traffic
        );
        std::fs::write(p.join("wm.toml"), "invalid").unwrap();
        assert!(Config::load(&p).is_err());
        Config::install(&p).unwrap();
        assert!(Config::load(&p).is_err());
        assert_eq!(c.wm.gap, 16);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn separators_repeat_but_unknown_modules_do_not_load() {
        let p = std::env::temp_dir().join(format!("winarchy-separators-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Config::install(&p).unwrap();
        let bar = |right: &str| {
            let text = include_str!("../config/defaults/bar.toml")
                .replace(
                    "center = [\"clock\"]",
                    "center = [\"separator\", \"clock\"]",
                )
                .replace(
                    "right = [\"battery\", \"cpu\", \"memory\", \"volume\", \"wifi\"]",
                    right,
                );
            std::fs::write(p.join("bar.toml"), text).unwrap();
        };
        bar("right = [\"battery\", \"separator\", \"cpu\", \"separator\", \"memory\"]");
        let c = Config::load(&p).unwrap();
        assert_eq!(c.bar.center, ["separator", "clock"]);
        assert_eq!(c.bar.right.iter().filter(|m| *m == "separator").count(), 2);
        bar("right = [\"divider\"]");
        assert!(Config::load(&p).is_err());
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn bundled_images_install_without_overwriting_user_assets() {
        let home =
            std::env::temp_dir().join(format!("winarchy-default-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        Config::install(&home).unwrap();
        for (name, bytes) in DEFAULT_ASSETS {
            let path = home.join(name);
            assert_eq!(&std::fs::read(&path).unwrap(), bytes);
            if winarchy_theme::pack::is_image(&path) {
                winarchy_theme::pack::decode(&path).unwrap();
            }
        }
        let catalog = winarchy_theme::preview::catalog(&home).unwrap();
        assert_eq!(catalog.len(), 2);
        for (id, count) in [("catppuccin-mocha", 4), ("catppuccin-latte", 2)] {
            let dir = winarchy_theme::pack::wallpaper_dir(&home, id).unwrap();
            assert_eq!(winarchy_theme::pack::images(&dir).unwrap().len(), count);
        }
        let preview = home.join("themes/catppuccin-mocha/preview.png");
        let wallpaper = home.join("themes/catppuccin-latte/wallpapers/1-color-fade.png");
        std::fs::write(&preview, "user preview").unwrap();
        std::fs::write(&wallpaper, "user wallpaper").unwrap();
        std::fs::write(home.join("wallpapers.json"), "user choices").unwrap();
        Config::install(&home).unwrap();
        assert_eq!(std::fs::read_to_string(preview).unwrap(), "user preview");
        assert_eq!(
            std::fs::read_to_string(wallpaper).unwrap(),
            "user wallpaper"
        );
        assert_eq!(
            std::fs::read_to_string(home.join("wallpapers.json")).unwrap(),
            "user choices"
        );
        std::fs::remove_dir_all(home).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn defaults_do_not_follow_asset_directory_links() {
        let home =
            std::env::temp_dir().join(format!("winarchy-default-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("themes")).unwrap();
        std::fs::create_dir(home.join("outside")).unwrap();
        std::os::unix::fs::symlink(home.join("outside"), home.join("themes/catppuccin-mocha"))
            .unwrap();
        assert!(Config::install(&home).is_err());
        assert_eq!(std::fs::read_dir(home.join("outside")).unwrap().count(), 0);
        std::fs::remove_dir_all(home).unwrap();
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
