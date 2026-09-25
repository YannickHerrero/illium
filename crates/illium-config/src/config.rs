use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Global {
    pub theme: String,
    #[serde(default)]
    pub screensaver: crate::screensaver::Screensaver,
    /// Read by the applications through `illium_theme`, not by the daemon.
    #[serde(default)]
    pub background_blur: bool,
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
    /// How clients of inactive workspaces leave the screen: `park` (moved
    /// off screen, the default) or `hide` (ShowWindow).
    #[serde(default = "default_conceal")]
    pub conceal: String,
    /// Windows session processes stopped once Illium has taken over the
    /// shell, as executable names. Windows restarts some of them on demand.
    #[serde(default)]
    pub stop_processes: Vec<String>,
}
impl Wm {
    pub fn park(&self) -> bool {
        self.conceal == "park"
    }
}
fn enabled() -> bool {
    true
}
fn default_conceal() -> String {
    "park".into()
}
fn default_border() -> i32 {
    2
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    pub enabled: bool,
    /// Display workspace numbers as Japanese kanji in the bar.
    #[serde(default)]
    pub japanese_workspace_numbers: bool,
    /// Empty selects Yu Gothic UI for kanji, JetBrains Mono otherwise.
    #[serde(default)]
    pub workspace_font_family: String,
    #[serde(default = "default_workspace_font_size")]
    pub workspace_font_size: i32,
    /// Omitted preserves automatic emphasis for active/kanji labels.
    #[serde(default)]
    pub workspace_font_weight: Option<i32>,
    pub position: String,
    pub height: i32,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    pub clock_format: String,
    /// Date half of the clock; empty uses the short weekday/day/month format.
    #[serde(default)]
    pub clock_date_format: String,
    /// Modules folded behind the `drawer` chevron, shown while it is expanded.
    #[serde(default)]
    pub drawer: Vec<String>,
}
fn default_workspace_font_size() -> i32 {
    13
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
pub use illium_theme::Theme;
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
pub const BUILTIN_MODULES: [&str; 11] = [
    "workspaces",
    "space",
    "window-title",
    "volume",
    "battery",
    "clock",
    "time",
    "cpu",
    "memory",
    "separator",
    "drawer",
];
const DEFAULTS: &[(&str, &str)] = &[
    (
        "illium.toml",
        include_str!("../../../config/defaults/illium.toml"),
    ),
    ("wm.toml", include_str!("../../../config/defaults/wm.toml")),
    (
        "bar.toml",
        include_str!("../../../config/defaults/bar.toml"),
    ),
    (
        "launcher.toml",
        include_str!("../../../config/defaults/launcher.toml"),
    ),
    (
        "apps.toml",
        include_str!("../../../config/defaults/apps.toml"),
    ),
    (
        "terminal.toml",
        include_str!("../../../config/defaults/terminal.toml"),
    ),
    (
        "keybindings.toml",
        include_str!("../../../config/defaults/keybindings.toml"),
    ),
    (
        "rules.toml",
        include_str!("../../../config/defaults/rules.toml"),
    ),
    (
        "themes/catppuccin-mocha.toml",
        include_str!("../../../config/themes/catppuccin-mocha.toml"),
    ),
    (
        "themes/catppuccin-latte.toml",
        include_str!("../../../config/themes/catppuccin-latte.toml"),
    ),
    (
        "themes/dynamic-dark.toml",
        include_str!("../../../config/themes/dynamic-dark.toml"),
    ),
    (
        "themes/dynamic-light.toml",
        include_str!("../../../config/themes/dynamic-light.toml"),
    ),
    (
        "applets/weather/applet.toml",
        include_str!("../../../config/applets/weather/applet.toml"),
    ),
    (
        "applets/weather/weather.ps1",
        include_str!("../../../config/applets/weather/weather.ps1"),
    ),
    (
        "applets/weather/view.slint",
        include_str!("../../../config/applets/weather/view.slint"),
    ),
    (
        "applets/weather/icon.svg",
        include_str!("../../../config/applets/weather/icon.svg"),
    ),
    (
        "applets/weather/sun.svg",
        include_str!("../../../config/applets/weather/sun.svg"),
    ),
    (
        "applets/weather/cloud.svg",
        include_str!("../../../config/applets/weather/cloud.svg"),
    ),
    (
        "applets/weather/rain.svg",
        include_str!("../../../config/applets/weather/rain.svg"),
    ),
    (
        "applets/weather/snow.svg",
        include_str!("../../../config/applets/weather/snow.svg"),
    ),
    (
        "applets/wifi/applet.toml",
        include_str!("../../../config/applets/wifi/applet.toml"),
    ),
    (
        "applets/wifi/wifi.ps1",
        include_str!("../../../config/applets/wifi/wifi.ps1"),
    ),
    (
        "applets/wifi/view.slint",
        include_str!("../../../config/applets/wifi/view.slint"),
    ),
    (
        "applets/wifi/icon.svg",
        include_str!("../../../config/applets/wifi/icon.svg"),
    ),
    (
        "applets/wifi/lock.svg",
        include_str!("../../../config/applets/wifi/lock.svg"),
    ),
    (
        "applets/timezones/applet.toml",
        include_str!("../../../config/applets/timezones/applet.toml"),
    ),
    (
        "applets/timezones/view.slint",
        include_str!("../../../config/applets/timezones/view.slint"),
    ),
    (
        "applets/timezones/timezones.ps1",
        include_str!("../../../config/applets/timezones/timezones.ps1"),
    ),
    (
        "applets/calendar/applet.toml",
        include_str!("../../../config/applets/calendar/applet.toml"),
    ),
    (
        "applets/calendar/view.slint",
        include_str!("../../../config/applets/calendar/view.slint"),
    ),
    (
        "applets/volume/applet.toml",
        include_str!("../../../config/applets/volume/applet.toml"),
    ),
    (
        "applets/volume/view.slint",
        include_str!("../../../config/applets/volume/view.slint"),
    ),
    (
        "applets/volume/speaker.svg",
        include_str!("../../../config/applets/volume/speaker.svg"),
    ),
    (
        "applets/volume/speaker-muted.svg",
        include_str!("../../../config/applets/volume/speaker-muted.svg"),
    ),
    (
        "applets/volume/output.svg",
        include_str!("../../../config/applets/volume/output.svg"),
    ),
    (
        "applets/volume/input.svg",
        include_str!("../../../config/applets/volume/input.svg"),
    ),
    (
        "applets/volume/source.svg",
        include_str!("../../../config/applets/volume/source.svg"),
    ),
    (
        "applets/battery/applet.toml",
        include_str!("../../../config/applets/battery/applet.toml"),
    ),
    (
        "applets/battery/view.slint",
        include_str!("../../../config/applets/battery/view.slint"),
    ),
    (
        "applets/_template/applet.toml",
        include_str!("../../../config/applets/_template/applet.toml"),
    ),
    (
        "applets/_template/_template.ps1",
        include_str!("../../../config/applets/_template/_template.ps1"),
    ),
    (
        "applets/_template/view.slint",
        include_str!("../../../config/applets/_template/view.slint"),
    ),
    (
        "applets/_template/icon.svg",
        include_str!("../../../config/applets/_template/icon.svg"),
    ),
];
// Bundled data, not runtime downloads. Keep existing installed files intact.
const DEFAULT_ASSETS: &[(&str, &[u8])] = &[
    (
        "themes/catppuccin-mocha/preview.png",
        include_bytes!("../../../config/themes/catppuccin-mocha/preview.png"),
    ),
    (
        "themes/dynamic-dark/preview.png",
        include_bytes!("../../../config/themes/dynamic-dark/preview.png"),
    ),
    (
        "themes/dynamic-light/preview.png",
        include_bytes!("../../../config/themes/dynamic-light/preview.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/1-totoro.png",
        include_bytes!("../../../config/themes/catppuccin-mocha/wallpapers/1-totoro.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/2-waves.png",
        include_bytes!("../../../config/themes/catppuccin-mocha/wallpapers/2-waves.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/3-blue-eye.png",
        include_bytes!("../../../config/themes/catppuccin-mocha/wallpapers/3-blue-eye.png"),
    ),
    (
        "themes/catppuccin-mocha/wallpapers/omarchy.png",
        include_bytes!("../../../config/themes/catppuccin-mocha/wallpapers/omarchy.png"),
    ),
    (
        "themes/catppuccin-mocha/SOURCES.md",
        include_bytes!("../../../config/themes/catppuccin-mocha/SOURCES.md"),
    ),
    (
        "themes/catppuccin-mocha/LICENSE",
        include_bytes!("../../../config/themes/catppuccin-mocha/LICENSE"),
    ),
    (
        "themes/catppuccin-latte/preview.png",
        include_bytes!("../../../config/themes/catppuccin-latte/preview.png"),
    ),
    (
        "themes/catppuccin-latte/wallpapers/1-color-fade.png",
        include_bytes!("../../../config/themes/catppuccin-latte/wallpapers/1-color-fade.png"),
    ),
    (
        "themes/catppuccin-latte/wallpapers/omarchy.png",
        include_bytes!("../../../config/themes/catppuccin-latte/wallpapers/omarchy.png"),
    ),
    (
        "themes/catppuccin-latte/SOURCES.md",
        include_bytes!("../../../config/themes/catppuccin-latte/SOURCES.md"),
    ),
    (
        "themes/catppuccin-latte/LICENSE",
        include_bytes!("../../../config/themes/catppuccin-latte/LICENSE"),
    ),
];
fn parse<T: serde::de::DeserializeOwned>(home: &Path, name: &str) -> Result<T, String> {
    let bytes = crate::files::read_config(&home.join(name))?;
    let s = std::str::from_utf8(&bytes).map_err(|e| format!("{name}: {e}"))?;
    toml::from_str(s).map_err(|e| format!("{name}: {e}"))
}
impl Config {
    pub fn home() -> PathBuf {
        illium_theme::config_home()
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
        crate::files::create_directory(&home.join("wallpapers"))?;
        crate::files::create_directory(&home.join("wallpapers/dynamic"))?;
        Ok(())
    }
    pub fn load(home: &Path) -> Result<Self, String> {
        crate::plugins::ensure_ready(home)?;
        Self::load_for_plugin_transaction(home)
    }
    pub(crate) fn load_for_plugin_transaction(home: &Path) -> Result<Self, String> {
        // Apply the same bounds at startup/reload as in the directory watcher.
        crate::files::snapshot_for_plugin_transaction(home)?;
        crate::applets::disabled(home)?;
        let global: Global = parse(home, "illium.toml")?;
        if !illium_theme::valid_name(&global.theme) {
            return Err("invalid theme name".into());
        }
        let mut c = Self {
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
        if !["park", "hide"].contains(&c.wm.conceal.as_str()) {
            return Err("wm: conceal must be park or hide".into());
        }
        // The names reach taskkill as arguments; keep them to bare filenames.
        if c.wm.stop_processes.iter().any(|n| {
            n.is_empty()
                || !n
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        }) {
            return Err("wm: stop_processes accepts executable names only".into());
        }
        if !(16..=100).contains(&c.bar.height)
            || !["top", "bottom"].contains(&c.bar.position.as_str())
        {
            return Err("bar: invalid height or position".into());
        }
        if !(6..=48).contains(&c.bar.workspace_font_size)
            || c.bar
                .workspace_font_weight
                .is_some_and(|weight| !(100..=900).contains(&weight))
        {
            return Err(
                "bar: workspace_font_size must be 6..48 and workspace_font_weight 100..900".into(),
            );
        }
        if !(200..=2000).contains(&c.launcher.width) || !(1..=30).contains(&c.launcher.max_results)
        {
            return Err("launcher: invalid dimensions".into());
        }
        crate::keyboard::parse(&c.keys)?;
        let drawers = [&c.bar.left, &c.bar.center, &c.bar.right]
            .iter()
            .flat_map(|modules| modules.iter())
            .filter(|module| *module == "drawer")
            .count();
        if drawers > 1 {
            return Err("bar: drawer may be listed once".into());
        }
        if (drawers == 1) == c.bar.drawer.is_empty() {
            return Err(
                "bar: drawer needs both a drawer entry in a section and a non-empty drawer list"
                    .into(),
            );
        }
        for (index, modules) in [&c.bar.left, &c.bar.center, &c.bar.right, &c.bar.drawer]
            .iter()
            .enumerate()
        {
            for module in *modules {
                if module == "workspaces" && index != 0 || module == "drawer" && index == 3 {
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
        crate::clock::validate(&c.bar.clock_date_format)
            .map_err(|e| format!("clock_date_format: {e}"))?;
        c.theme.validate()?;
        // Runtime state is disposable, not user configuration. A damaged state
        // must not prevent startup: the wallpaper worker will regenerate it.
        if let Err(error) = illium_theme::dynamic::apply(home, &c.global.theme, &mut c.theme) {
            tracing::warn!(%error, "using installed dynamic fallback until wallpaper is prepared");
        }
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
        let p = std::env::temp_dir().join(format!("illium-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Config::install(&p).unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.global.theme, "catppuccin-mocha");
        assert_eq!(c.keys.keybindings.len(), 62);
        assert_eq!(c.keys.keybindings["Alt+B"], "spawn browser");
        assert_eq!(c.apps.apps["browser"], "illium-browser.exe");
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
    fn japanese_workspace_numbers_are_optional() {
        let original = include_str!("../../../config/defaults/bar.toml");
        let legacy = original.replace("japanese_workspace_numbers = true", "");
        assert!(
            !toml::from_str::<Bar>(&legacy)
                .unwrap()
                .japanese_workspace_numbers
        );
        assert!(
            toml::from_str::<Bar>(original)
                .unwrap()
                .japanese_workspace_numbers
        );
        let disabled = original.replace(
            "japanese_workspace_numbers = true",
            "japanese_workspace_numbers = false",
        );
        assert!(
            !toml::from_str::<Bar>(&disabled)
                .unwrap()
                .japanese_workspace_numbers
        );
        let invalid = original.replace(
            "japanese_workspace_numbers = true",
            "japanese_workspace_numbers = \"true\"",
        );
        assert!(toml::from_str::<Bar>(&invalid).is_err());
    }
    #[test]
    fn workspace_typography_defaults_and_validation() {
        let home =
            std::env::temp_dir().join(format!("illium-workspace-font-{}", std::process::id()));
        Config::install(&home).unwrap();
        let original = std::fs::read_to_string(home.join("bar.toml")).unwrap();
        let legacy = original
            .lines()
            .filter(|line| !line.starts_with("workspace_font_"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(home.join("bar.toml"), &legacy).unwrap();
        let bar = Config::load(&home).unwrap().bar;
        assert!(bar.workspace_font_family.is_empty());
        assert_eq!(bar.workspace_font_size, 13);
        assert_eq!(bar.workspace_font_weight, None);
        for settings in [
            "workspace_font_size = 0",
            "workspace_font_size = 49",
            "workspace_font_weight = 99",
            "workspace_font_weight = 901",
            "workspace_font_weight = \"bold\"",
        ] {
            std::fs::write(home.join("bar.toml"), format!("{legacy}\n{settings}\n")).unwrap();
            assert!(Config::load(&home).is_err(), "{settings}");
        }
        std::fs::write(home.join("bar.toml"), format!("{legacy}\nworkspace_font_family = \"Yu Gothic UI\"\nworkspace_font_size = 16\nworkspace_font_weight = 600\n")).unwrap();
        let bar = Config::load(&home).unwrap().bar;
        assert_eq!(bar.workspace_font_family, "Yu Gothic UI");
        assert_eq!(bar.workspace_font_size, 16);
        assert_eq!(bar.workspace_font_weight, Some(600));
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn secondary_clock_date_is_optional_and_validated() {
        let home = std::env::temp_dir().join(format!("illium-clock-date-{}", std::process::id()));
        Config::install(&home).unwrap();
        let original = std::fs::read_to_string(home.join("bar.toml")).unwrap();
        assert!(
            Config::load(&home)
                .unwrap()
                .bar
                .clock_date_format
                .is_empty()
        );
        std::fs::write(
            home.join("bar.toml"),
            format!("{original}\nclock_date_format = \"%a %d %b\"\n"),
        )
        .unwrap();
        assert_eq!(
            Config::load(&home).unwrap().bar.clock_date_format,
            "%a %d %b"
        );
        std::fs::write(
            home.join("bar.toml"),
            format!("{original}\nclock_date_format = \"%Q\"\n"),
        )
        .unwrap();
        assert!(Config::load(&home).is_err());
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn separators_repeat_but_unknown_modules_do_not_load() {
        let p = std::env::temp_dir().join(format!("illium-separators-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Config::install(&p).unwrap();
        let bar = |right: &str| {
            let text = include_str!("../../../config/defaults/bar.toml")
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
    fn drawer_is_listed_once_with_a_matching_module_list() {
        let p = std::env::temp_dir().join(format!("illium-drawer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Config::install(&p).unwrap();
        let bar = |right: &str, drawer: &str| {
            let text = include_str!("../../../config/defaults/bar.toml").replace(
                "right = [\"battery\", \"cpu\", \"memory\", \"volume\", \"wifi\"]",
                &format!("{right}\n{drawer}"),
            );
            std::fs::write(p.join("bar.toml"), text).unwrap();
        };
        bar(
            "right = [\"battery\", \"drawer\", \"volume\"]",
            "drawer = [\"cpu\", \"memory\", \"wifi\"]",
        );
        let c = Config::load(&p).unwrap();
        assert_eq!(c.bar.drawer, ["cpu", "memory", "wifi"]);
        assert!(c.bar.right.contains(&"drawer".to_string()));
        bar("right = [\"battery\"]", "");
        assert!(Config::load(&p).unwrap().bar.drawer.is_empty());
        for (right, drawer) in [
            ("right = [\"drawer\"]", ""),
            ("right = [\"battery\"]", "drawer = [\"cpu\"]"),
            ("right = [\"drawer\", \"drawer\"]", "drawer = [\"cpu\"]"),
            ("right = [\"drawer\"]", "drawer = [\"drawer\"]"),
            ("right = [\"drawer\"]", "drawer = [\"workspaces\"]"),
            ("right = [\"drawer\"]", "drawer = [\"nope\"]"),
        ] {
            bar(right, drawer);
            assert!(Config::load(&p).is_err(), "{right} / {drawer}");
        }
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn dynamic_runtime_corruption_does_not_invalidate_installed_configuration() {
        let home =
            std::env::temp_dir().join(format!("illium-dynamic-config-{}", std::process::id()));
        Config::install(&home).unwrap();
        std::fs::write(home.join("illium.toml"), "theme = 'dynamic-light'").unwrap();
        std::fs::write(home.join(illium_theme::dynamic::FILE), "broken").unwrap();
        assert_eq!(
            Config::load(&home).unwrap().theme,
            illium_theme::Theme::load(&home, "dynamic-light").unwrap()
        );
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn bundled_images_install_without_overwriting_user_assets() {
        let home =
            std::env::temp_dir().join(format!("illium-default-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        Config::install(&home).unwrap();
        for (name, bytes) in DEFAULT_ASSETS {
            let path = home.join(name);
            assert_eq!(&std::fs::read(&path).unwrap(), bytes);
            if illium_theme::pack::is_image(&path) {
                illium_theme::pack::decode(&path).unwrap();
            }
        }
        let catalog = illium_theme::preview::catalog(&home).unwrap();
        assert_eq!(catalog.len(), 4);
        for id in ["dynamic-dark", "dynamic-light"] {
            assert!(catalog.iter().any(|entry| entry.id == id));
            assert_eq!(
                illium_theme::pack::wallpaper_dir(&home, id).unwrap(),
                home.join("wallpapers/dynamic")
            );
        }
        assert!(home.join("wallpapers/dynamic").is_dir());
        for (id, count) in [("catppuccin-mocha", 4), ("catppuccin-latte", 2)] {
            let dir = illium_theme::pack::wallpaper_dir(&home, id).unwrap();
            assert_eq!(illium_theme::pack::images(&dir).unwrap().len(), count);
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
            std::env::temp_dir().join(format!("illium-default-links-{}", std::process::id()));
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
