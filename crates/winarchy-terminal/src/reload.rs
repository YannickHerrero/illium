//! OS directory notifications, not polling. File IO/parsing happens on one
//! worker; a single latest-value slot bounds reloads during editor save bursts.
use crate::config::Config;
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use winarchy_theme::Theme;
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
    path == home.join("winarchy.toml")
        || path == home.join("terminal.toml")
        || path == home.join(winarchy_theme::opacity::FILE)
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
    fn watches_only_terminal_and_palette_files() {
        let home = Path::new("config");
        for p in [
            "terminal.toml",
            winarchy_theme::opacity::FILE,
            "winarchy.toml",
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
