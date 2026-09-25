//! Bounded, event-driven palette updates. IO stays off application UI threads.
use crate::{Theme, opacity};
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

fn relevant(home: &Path, path: &Path) -> bool {
    path == home.join("illium.toml")
        || path == home.join(opacity::FILE)
        || path == home.join(crate::dynamic::FILE)
        || path == home.join("themes")
        || (path.parent() == Some(home.join("themes").as_path())
            && path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("toml")))
}

pub struct Subscription {
    _watcher: notify::RecommendedWatcher,
}

/// Keep the returned watcher alive with the window. Dropping it stops the worker.
/// Invalid palettes retain the last valid appearance. The callback runs on a worker.
pub fn watch(
    home: PathBuf,
    mut changed: impl FnMut(Theme) + Send + 'static,
) -> Result<Subscription, String> {
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::sync_channel(1);
    let root = home.clone();
    let initial = tx.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|e| {
            !matches!(e.kind, EventKind::Access(_)) && e.paths.iter().any(|p| relevant(&root, p))
        }) {
            let _ = tx.try_send(());
        }
    })
    .map_err(|e| e.to_string())?;
    // Also covers themes/ created or replaced after subscribing. Assets and logs
    // may trigger OS events but never cause reads or UI updates.
    watcher
        .watch(&home, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    let _ = initial.try_send(());
    std::thread::spawn(move || {
        let mut previous = None;
        while rx.recv().is_ok() {
            while rx.recv_timeout(Duration::from_millis(40)).is_ok() {}
            if let Ok(theme) = Theme::effective(&home)
                && previous.as_ref() != Some(&theme)
            {
                previous = Some(theme.clone());
                changed(theme);
            }
        }
    });
    Ok(Subscription { _watcher: watcher })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_palette_and_runtime_changes_are_relevant() {
        let home = Path::new("config");
        for p in [
            "illium.toml",
            opacity::FILE,
            crate::dynamic::FILE,
            "themes",
            "themes/mine.toml",
        ] {
            assert!(relevant(home, &home.join(p)));
        }
        for p in [
            "terminal.log",
            "background-opacity.state.tmp",
            "themes/mine/preview.png",
            "state.json",
        ] {
            assert!(!relevant(home, &home.join(p)));
        }
    }
    #[cfg(feature = "assets")]
    #[test]
    fn running_consumers_follow_dynamic_palettes_and_ignore_invalid_state() {
        let home = crate::tests::home();
        std::fs::write(home.join("illium.toml"), "theme = 'dynamic-dark'").unwrap();
        std::fs::write(
            home.join("themes/dynamic-dark.toml"),
            include_str!("../../../config/themes/dynamic-dark.toml"),
        )
        .unwrap();
        let (tx, rx) = mpsc::channel();
        let watcher = watch(home.clone(), move |theme| {
            let _ = tx.send(theme);
        })
        .unwrap();
        let receive = || rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let fallback = receive();
        let snapshot = crate::dynamic::prepare(
            &home,
            "a.png",
            &image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 50, 10, 255])),
        );
        crate::dynamic::publish(&home, Some(&snapshot)).unwrap();
        assert_eq!(receive(), snapshot.palettes.dark);
        opacity::set(&home, "dynamic-dark", 0.65).unwrap();
        assert_eq!(receive().background_opacity, 0.65);
        std::fs::write(home.join(crate::dynamic::FILE), "broken").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        assert!(Theme::effective(&home).is_err());
        crate::dynamic::publish(&home, None).unwrap();
        let mut expected = fallback;
        expected.background_opacity = 0.65;
        assert_eq!(receive(), expected);
        drop(watcher);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn running_consumers_follow_override_and_reset() {
        let home = crate::tests::home();
        std::fs::write(home.join("illium.toml"), "theme = 'mine'").unwrap();
        std::fs::write(home.join("themes/mine.toml"), crate::DEFAULT).unwrap();
        let (tx, rx) = mpsc::channel();
        let watcher = watch(home.clone(), move |theme| {
            let _ = tx.send(theme);
        })
        .unwrap();
        let receive = || {
            rx.recv_timeout(Duration::from_secs(5))
                .unwrap()
                .background_opacity
        };
        assert_eq!(receive(), 0.85);
        opacity::set(&home, "mine", 0.70).unwrap();
        assert_eq!(receive(), 0.70);
        assert_eq!(Theme::current(&home).background_opacity, 0.70);
        opacity::clear(&home).unwrap();
        assert_eq!(receive(), 0.85);
        drop(watcher);
        std::fs::remove_dir_all(home).unwrap();
    }
}
