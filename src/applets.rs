//! Applet discovery and manifest handling: a folder per applet under the
//! configuration home, with an `applet.toml`, an icon, a Slint view and a
//! data provider (a script printing JSON, or a built-in provider).
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
pub const MAX_APPLETS: usize = 64;
/// Bounds on what a provider may print and how long it may run.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(20);
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Monochrome icon shown in the bar, relative to the applet folder; may
    /// hold `{field}` placeholders so the provider picks the file.
    #[serde(default = "default_icon")]
    pub icon: String,
    /// Full command line; overrides `script`.
    pub command: Option<Vec<String>>,
    /// PowerShell script relative to the applet folder; defaults to `<name>.ps1`.
    pub script: Option<String>,
    /// `builtin:clock`, `builtin:system` or `builtin:volume` instead of a command.
    pub provider: Option<String>,
    /// Add native Wi-Fi traffic fields using the provider's `interface_guid`.
    #[serde(default)]
    pub wifi_traffic: bool,
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default)]
    pub popup: PopupSize,
    /// Bar text next to the icon, with `{field}` or `{a.b}` placeholders.
    pub label: Option<String>,
    /// Let the popup take keyboard focus (global Alt chords stop while it does).
    #[serde(default)]
    pub focusable: bool,
    /// Built-in module whose click opens this applet instead of its details;
    /// the applet then has no icon of its own.
    pub attach: Option<String>,
    /// Passed to the provider as WINARCHY_APPLET_<KEY> environment variables.
    #[serde(default)]
    pub settings: BTreeMap<String, toml::Value>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopupSize {
    pub width: i32,
    pub height: i32,
}
impl Default for PopupSize {
    fn default() -> Self {
        Self {
            width: 360,
            height: 240,
        }
    }
}
fn default_icon() -> String {
    "icon.svg".into()
}
fn default_interval() -> String {
    "1m".into()
}
#[derive(Clone)]
pub struct Applet {
    pub name: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn dir(home: &Path, name: &str) -> PathBuf {
    home.join("applets").join(name)
}
/// The applet folder for `name`, if it holds a manifest.
pub fn exists(home: &Path, name: &str) -> bool {
    valid_name(name) && dir(home, name).join("applet.toml").is_file()
}
pub fn load(home: &Path, name: &str) -> Result<Applet, String> {
    if !valid_name(name) {
        return Err(format!("applet {name}: invalid name"));
    }
    let dir = dir(home, name);
    let bytes = crate::files::read_config(&dir.join("applet.toml"))
        .map_err(|e| format!("applet {name}: {e}"))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("applet {name}: {e}"))?;
    let manifest: Manifest = toml::from_str(text).map_err(|e| format!("applet {name}: {e}"))?;
    interval(&manifest.interval).map_err(|e| format!("applet {name}: {e}"))?;
    if !(120..=2000).contains(&manifest.popup.width)
        || !(60..=1600).contains(&manifest.popup.height)
    {
        return Err(format!("applet {name}: popup size out of range"));
    }
    if let Some(p) = &manifest.provider
        && !["builtin:clock", "builtin:system", "builtin:volume"].contains(&p.as_str())
    {
        return Err(format!("applet {name}: unknown provider {p}"));
    }
    if let Some(module) = &manifest.attach
        && (!crate::config::BUILTIN_MODULES.contains(&module.as_str())
            || ["workspaces", "separator"].contains(&module.as_str()))
    {
        return Err(format!("applet {name}: attach must name a built-in module"));
    }
    Ok(Applet {
        name: name.to_owned(),
        dir,
        manifest,
    })
}
/// Applets referenced by the bar sections, in order, plus the ones attached to
/// a built-in module that appears in a section; no duplicates.
pub fn referenced(home: &Path, sections: &[&Vec<String>]) -> Vec<Result<Applet, String>> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    let mut push = |name: &str, out: &mut Vec<Result<Applet, String>>| {
        if seen.iter().any(|s| s == name) || out.len() >= MAX_APPLETS {
            return;
        }
        seen.push(name.to_owned());
        out.push(load(home, name));
    };
    let modules: Vec<&String> = sections.iter().flat_map(|s| s.iter()).collect();
    for name in &modules {
        if !crate::config::BUILTIN_MODULES.contains(&name.as_str()) {
            push(name, &mut out);
        }
    }
    for name in folders(home) {
        if let Ok(a) = load(home, &name)
            && let Some(module) = &a.manifest.attach
            && modules.contains(&module)
        {
            push(&name, &mut out);
        }
    }
    out
}
/// Applet folder names under `applets/`, bounded and sorted.
pub fn folders(home: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(home.join("applets")) else {
        return vec![];
    };
    let mut names: Vec<String> = entries
        .flatten()
        .take(256)
        .filter(|e| e.path().join("applet.toml").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    names
}
/// `30s`, `10m`, `2h`; at least one second.
pub fn interval(text: &str) -> Result<Duration, String> {
    let (digits, unit) = text.split_at(text.trim_end_matches(char::is_alphabetic).len());
    let n: u64 = digits
        .parse()
        .map_err(|_| format!("invalid interval {text:?}"))?;
    let seconds = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        _ => return Err(format!("invalid interval {text:?}: use s, m or h")),
    };
    if seconds == 0 {
        return Err("interval must be at least 1s".into());
    }
    Ok(Duration::from_secs(seconds))
}
/// Command line for the provider: explicit `command`, or PowerShell running `script`.
pub fn command(applet: &Applet) -> Vec<String> {
    if let Some(c) = &applet.manifest.command {
        return c.clone();
    }
    let script = applet
        .manifest
        .script
        .clone()
        .unwrap_or_else(|| format!("{}.ps1", applet.name));
    vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        applet.dir.join(script).to_string_lossy().into_owned(),
    ]
}
/// Environment passed to the provider from `[settings]`.
pub fn environment(applet: &Applet) -> Vec<(String, String)> {
    applet
        .manifest
        .settings
        .iter()
        .map(|(k, v)| {
            let value = match v {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (format!("WINARCHY_APPLET_{}", k.to_ascii_uppercase()), value)
        })
        .collect()
}
/// The bar icon file named by the manifest's `icon` once its `{field}`
/// placeholders are filled: a plain file name inside the applet folder, or
/// nothing when the data does not name one yet.
pub fn icon_file(template: &str, data: &serde_json::Value) -> Option<String> {
    let name = label(template, data);
    let (stem, extension) = name.rsplit_once('.')?;
    let plain = |s: &str| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    (plain(stem) && plain(extension)).then_some(name)
}
/// Fills `{field}` and `{a.b}` placeholders from the provider's JSON.
pub fn label(template: &str, data: &serde_json::Value) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let path = &rest[start + 1..start + end];
        let mut value = data;
        for key in path.split('.') {
            value = value.get(key).unwrap_or(&serde_json::Value::Null);
        }
        match value {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Null => {}
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64()
                    && f.fract() != 0.0
                {
                    out.push_str(&format!("{f:.1}"));
                } else {
                    out.push_str(&n.to_string());
                }
            }
            other => out.push_str(&other.to_string()),
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intervals() {
        assert_eq!(interval("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(interval("10m").unwrap(), Duration::from_secs(600));
        assert_eq!(interval("2h").unwrap(), Duration::from_secs(7200));
        for bad in ["0s", "10", "5d", "", "m"] {
            assert!(interval(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn icon_files_are_plain_names_in_the_folder() {
        let data = serde_json::json!({"icon": "mic-off.svg", "bad": "../x.svg", "none": ""});
        assert_eq!(icon_file("icon.svg", &data).as_deref(), Some("icon.svg"));
        assert_eq!(icon_file("{icon}", &data).as_deref(), Some("mic-off.svg"));
        for template in [
            "{bad}",
            "{none}",
            "{missing}",
            "icon",
            "sub/icon.svg",
            ".svg",
        ] {
            assert_eq!(icon_file(template, &data), None, "{template}");
        }
    }
    #[test]
    fn labels() {
        let data: serde_json::Value = serde_json::from_str(
            r#"{"temperature": 21.6, "wind": {"speed": 18}, "place": "Paris", "ok": true}"#,
        )
        .unwrap();
        assert_eq!(label("{temperature}°", &data), "21.6°");
        assert_eq!(label("{wind.speed} km/h", &data), "18 km/h");
        assert_eq!(label("{place} {missing}!", &data), "Paris !");
        assert_eq!(label("{ok} {unterminated", &data), "true {unterminated");
    }
    #[test]
    fn names_and_manifests() {
        assert!(valid_name("weather") && valid_name("wifi-2") && valid_name("my_app"));
        assert!(!valid_name("") && !valid_name("../x") && !valid_name("a b"));
        let home = std::env::temp_dir().join(format!("winarchy-applets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let dir = home.join("applets/sample");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("applet.toml"),
            "interval = \"5m\"\nlabel = \"{value}\"\n[settings]\ncity = \"Lyon\"\ncount = 3\n",
        )
        .unwrap();
        assert!(exists(&home, "sample") && !exists(&home, "other"));
        assert_eq!(folders(&home), vec!["sample".to_owned()]);
        std::fs::write(
            dir.join("applet.toml"),
            "provider = \"builtin:clock\"\nattach = \"clock\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("applet.toml"),
            "provider = \"builtin:volume\"\nattach = \"volume\"\n",
        )
        .unwrap();
        assert!(load(&home, "sample").is_ok());
        std::fs::write(dir.join("applet.toml"), "provider = \"builtin:other\"\n").unwrap();
        assert!(load(&home, "sample").is_err());
        std::fs::write(
            dir.join("applet.toml"),
            "provider = \"builtin:clock\"\nattach = \"clock\"\n",
        )
        .unwrap();
        let clock = vec!["clock".to_owned()];
        let loaded = referenced(&home, &[&clock]);
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0].as_ref().unwrap().manifest.attach.as_deref(),
            Some("clock")
        );
        assert!(referenced(&home, &[&vec!["cpu".to_owned()]]).is_empty());
        std::fs::write(dir.join("applet.toml"), "attach = \"workspaces\"\n").unwrap();
        assert!(load(&home, "sample").is_err());
        std::fs::write(
            dir.join("applet.toml"),
            "interval = \"5m\"\nlabel = \"{value}\"\n[settings]\ncity = \"Lyon\"\ncount = 3\n",
        )
        .unwrap();
        let a = load(&home, "sample").unwrap();
        assert_eq!(a.manifest.icon, "icon.svg");
        assert_eq!(a.manifest.popup.width, 360);
        assert!(command(&a).last().unwrap().ends_with("sample.ps1"));
        assert_eq!(
            environment(&a),
            vec![
                ("WINARCHY_APPLET_CITY".to_owned(), "Lyon".to_owned()),
                ("WINARCHY_APPLET_COUNT".to_owned(), "3".to_owned()),
            ]
        );
        std::fs::write(dir.join("applet.toml"), "interval = \"never\"\n").unwrap();
        assert!(load(&home, "sample").is_err());
        std::fs::remove_dir_all(home).unwrap();
    }
}
