//! Per-theme wallpaper memory, separate from distributed palettes and placement state.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use winarchy_theme::pack::plain_name;

#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selections {
    /// Missing entry: first available image. Explicit null: solid theme background.
    themes: BTreeMap<String, Option<String>>,
}
impl Selections {
    pub fn load(home: &Path) -> Self {
        let loaded = crate::files::read_config(&home.join("wallpapers.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok());
        let Some(mut state) = loaded else {
            return Self::default();
        };
        state.themes.retain(|theme, image| {
            winarchy_theme::valid_name(theme)
                && plain_name(theme)
                && image.as_ref().is_none_or(|name| plain_name(name))
        });
        if state.themes.len() > 256 {
            return Self::default();
        }
        state
    }
    pub fn save_choice(
        &mut self,
        home: &Path,
        theme: &str,
        name: Option<String>,
    ) -> Result<(), String> {
        if !winarchy_theme::valid_name(theme)
            || !plain_name(theme)
            || name.as_ref().is_some_and(|name| !plain_name(name))
        {
            return Err("invalid wallpaper selection".into());
        }
        let old = self.themes.insert(theme.into(), name);
        let result = (|| {
            let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
            if self.themes.len() > 256 || json.len() > crate::files::MAX_CONFIG_BYTES {
                return Err("wallpaper selections exceed configuration limits".into());
            }
            crate::state::State::save(&json, &home.join("wallpapers.json"))
        })();
        if result.is_err() {
            match old {
                Some(value) => {
                    self.themes.insert(theme.into(), value);
                }
                None => {
                    self.themes.remove(theme);
                }
            }
        }
        result
    }
    /// Try the remembered image first, then the others. Missing/corrupt images
    /// must not make an otherwise valid theme unusable.
    pub fn candidates(&self, theme: &str, names: &[String]) -> Vec<String> {
        match self.themes.get(theme) {
            Some(None) => vec![],
            Some(Some(selected)) => names
                .iter()
                .filter(|n| *n == selected)
                .chain(names.iter().filter(|n| *n != selected))
                .cloned()
                .collect(),
            None => names.to_vec(),
        }
    }
}
/// One full cycle, excluding no files: callers can skip unreadable images.
pub fn next_candidates(names: &[String], current: Option<&str>) -> Vec<String> {
    let start = current
        .and_then(|name| names.iter().position(|n| n == name))
        .map_or(0, |index| index + 1);
    names
        .iter()
        .cycle()
        .skip(start)
        .take(names.len())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_cycle_and_missing_image() {
        let names = vec!["a.png".into(), "b.jpg".into(), "c.jpg".into()];
        assert_eq!(next_candidates(&names, None), names);
        assert_eq!(next_candidates(&names, Some("c.jpg")), names);
        assert_eq!(
            next_candidates(&names, Some("a.png")),
            ["b.jpg", "c.jpg", "a.png"]
        );
        assert!(next_candidates(&[], None).is_empty());
        let state = Selections {
            themes: BTreeMap::from([
                ("dark".into(), Some("b.jpg".into())),
                ("light".into(), None),
                ("missing".into(), Some("gone.jpg".into())),
            ]),
        };
        assert_eq!(
            state.candidates("dark", &names),
            ["b.jpg", "a.png", "c.jpg"]
        );
        assert_eq!(state.candidates("missing", &names), names);
        assert_eq!(state.candidates("new", &names), names);
        assert!(state.candidates("light", &names).is_empty());
    }
    #[test]
    fn image_and_selection_edits_do_not_reload_configuration() {
        let home =
            std::env::temp_dir().join(format!("winarchy-wallpaper-watch-{}", std::process::id()));
        crate::config::Config::install(&home).unwrap();
        let dir = winarchy_theme::pack::wallpaper_dir(&home, "catppuccin-mocha").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let before = crate::files::snapshot(&home).unwrap();
        let fingerprint = winarchy_theme::pack::fingerprint(&home, "catppuccin-mocha").unwrap();
        std::fs::write(dir.join("new.jpg"), "not decoded by watcher").unwrap();
        assert_ne!(
            winarchy_theme::pack::fingerprint(&home, "catppuccin-mocha").unwrap(),
            fingerprint
        );
        Selections::default()
            .save_choice(&home, "catppuccin-mocha", None)
            .unwrap();
        assert_eq!(crate::files::snapshot(&home).unwrap(), before);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn independent_choices_survive_restart() {
        let home = std::env::temp_dir().join(format!("winarchy-wallpapers-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let mut state = Selections::default();
        state
            .save_choice(&home, "dark", Some("with spaces.jpg".into()))
            .unwrap();
        state.save_choice(&home, "light", None).unwrap();
        assert_eq!(Selections::load(&home), state);
        assert!(
            state
                .save_choice(&home, "dark", Some("../escape.jpg".into()))
                .is_err()
        );
        assert_eq!(Selections::load(&home), state);
        std::fs::write(home.join("wallpapers.json"), "invalid").unwrap();
        assert_eq!(Selections::load(&home), Selections::default());
        std::fs::remove_dir_all(home).unwrap();
    }
}
