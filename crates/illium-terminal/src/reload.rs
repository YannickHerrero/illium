//! OS directory notifications, not polling. File IO/parsing happens on one
//! worker; a single latest-value slot bounds reloads during editor save bursts.
use crate::config::Config;
use illium_theme::Theme;
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub config: Config,
    pub theme: Theme,
}
impl Snapshot {
    pub fn load(home: &Path) -> Result<Self, String> {
        Ok(Self {
            config: Config::load(home)?,
            theme: Theme::effective(home)?,
        })
    }
}
pub type Slot = Arc<Mutex<Option<Result<Snapshot, String>>>>;
fn relevant(home: &Path, path: &Path) -> bool {
    path == home.join("illium.toml")
        || path == home.join("terminal.toml")
        || path == home.join(illium_theme::opacity::FILE)
        || path == home.join(illium_theme::dynamic::FILE)
        || path == home.join("themes")
        || (path.parent() == Some(home.join("themes").as_path())
            && path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("toml")))
}
pub fn watch(home: PathBuf, wake: impl Fn() + Send + 'static) -> Result<Slot, String> {
    // Ensure a directory exists to watch even for independent first use. No
    // default theme/config is written over the user's selected configuration.
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::sync_channel(1);
    let root = home.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|e| {
            !matches!(e.kind, EventKind::Access(_)) && e.paths.iter().any(|p| relevant(&root, p))
        }) {
            let _ = tx.try_send(());
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(&home, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    let themes = home.join("themes");
    if themes.is_dir() {
        watcher
            .watch(&themes, RecursiveMode::NonRecursive)
            .map_err(|e| e.to_string())?;
    }
    let slot = Arc::new(Mutex::new(None));
    let result = slot.clone();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(40)).is_ok() {}
            // Re-arm after directory creation/replacement. Duplicate watch is
            // harmless; previews/wallpapers below it are never traversed.
            let _ = watcher.watch(&themes, RecursiveMode::NonRecursive);
            *slot.lock().unwrap() = Some(Snapshot::load(&home));
            wake();
        }
    });
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_applies_shared_override_without_changing_terminal_preferences() {
        let home = std::env::temp_dir().join(format!("terminal-opacity-{}", std::process::id()));
        std::fs::create_dir_all(home.join("themes")).unwrap();
        std::fs::write(home.join("illium.toml"), "theme = 'test'").unwrap();
        std::fs::write(
            home.join("themes/test.toml"),
            include_str!("../../../config/themes/catppuccin-mocha.toml"),
        )
        .unwrap();
        let before = Snapshot::load(&home).unwrap();
        illium_theme::opacity::set(&home, "test", 0.65).unwrap();
        let after = Snapshot::load(&home).unwrap();
        assert_eq!(after.theme.background_opacity, 0.65);
        assert_eq!(before.config, after.config);
        assert_eq!(Theme::load(&home, "test").unwrap().background_opacity, 0.85);
        illium_theme::opacity::clear(&home).unwrap();
        assert_eq!(
            Snapshot::load(&home).unwrap().theme.background_opacity,
            0.85
        );
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn watches_only_terminal_and_palette_files() {
        let home = Path::new("config");
        for p in [
            "terminal.toml",
            illium_theme::opacity::FILE,
            illium_theme::dynamic::FILE,
            "illium.toml",
            "themes",
            "themes/test.toml",
        ] {
            assert!(relevant(home, &home.join(p)));
        }
        for p in [
            "terminal.log",
            "terminal.log.1",
            "themes/test/wallpapers/one.png",
            "themes/test/preview.png",
            "state.json",
        ] {
            assert!(!relevant(home, &home.join(p)));
        }
    }
}
