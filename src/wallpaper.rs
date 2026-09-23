//! Per-theme wallpaper memory, separate from distributed palettes and placement state.
pub mod loader;
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
        let theme = winarchy_theme::dynamic::selection_key(theme);
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
        match self
            .themes
            .get(winarchy_theme::dynamic::selection_key(theme))
        {
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
/// Publish only fully prepared requests. If runtime publication fails, restore
/// the saved selection before returning; the shell keeps its old image/colors.
pub fn commit_choice(
    home: &Path,
    theme: &str,
    name: Option<String>,
    snapshot: Option<&winarchy_theme::dynamic::Snapshot>,
    persist: bool,
) -> Result<(), String> {
    let dynamic = winarchy_theme::dynamic::is_dynamic(theme);
    let path = home.join("wallpapers.json");
    let previous = if dynamic && persist && path.try_exists().map_err(|e| e.to_string())? {
        Some(crate::files::read_config(&path)?)
    } else {
        None
    };
    if persist {
        Selections::load(home).save_choice(home, theme, name)?;
    }
    if dynamic && let Err(error) = winarchy_theme::dynamic::publish(home, snapshot) {
        if persist {
            let rollback = match previous {
                Some(bytes) => std::str::from_utf8(&bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|text| crate::state::State::save(text, &path)),
                None => std::fs::remove_file(&path).map_err(|e| e.to_string()),
            };
            if let Err(rollback) = rollback {
                return Err(format!("{error}; choice rollback failed: {rollback}"));
            }
        }
        return Err(error);
    }
    Ok(())
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
    fn dynamic_variants_share_choice_but_not_static_themes() {
        let home =
            std::env::temp_dir().join(format!("winarchy-dynamic-choice-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let names = vec!["a.png".into(), "b.png".into()];
        let mut state = Selections::default();
        state
            .save_choice(&home, "dynamic-light", Some("b.png".into()))
            .unwrap();
        let mut state = Selections::load(&home);
        assert_eq!(state.candidates("dynamic-dark", &names), ["b.png", "a.png"]);
        assert_eq!(state.candidates("static", &names), names);
        state.save_choice(&home, "dynamic-dark", None).unwrap();
        assert!(
            Selections::load(&home)
                .candidates("dynamic-light", &names)
                .is_empty()
        );
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn dynamic_publication_rolls_back_choice_on_failure_and_clear_uses_fallback() {
        let home = std::env::temp_dir().join(format!("dynamic-commit-{}", std::process::id()));
        crate::config::Config::install(&home).unwrap();
        let before_config = crate::files::snapshot(&home).unwrap();
        let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([30, 90, 160, 255]));
        let snapshot = winarchy_theme::dynamic::prepare(&home, "new.png", &pixels);
        Selections::default()
            .save_choice(&home, "dynamic-dark", Some("old.png".into()))
            .unwrap();
        let before = std::fs::read(home.join("wallpapers.json")).unwrap();
        // A directory at the publication target makes atomic rename fail.
        std::fs::create_dir(home.join(winarchy_theme::dynamic::FILE)).unwrap();
        assert!(
            commit_choice(
                &home,
                "dynamic-dark",
                Some("new.png".into()),
                Some(&snapshot),
                true
            )
            .is_err()
        );
        assert_eq!(std::fs::read(home.join("wallpapers.json")).unwrap(), before);
        std::fs::remove_dir(home.join(winarchy_theme::dynamic::FILE)).unwrap();
        commit_choice(
            &home,
            "dynamic-dark",
            Some("new.png".into()),
            Some(&snapshot),
            true,
        )
        .unwrap();
        assert!(home.join(winarchy_theme::dynamic::FILE).is_file());
        assert_eq!(crate::files::snapshot(&home).unwrap(), before_config);
        commit_choice(&home, "dynamic-light", None, None, true).unwrap();
        assert!(!home.join(winarchy_theme::dynamic::FILE).exists());
        assert!(
            Selections::load(&home)
                .candidates("dynamic-dark", &["new.png".into()])
                .is_empty()
        );
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

/// Downsampling factor of the exposé backdrop.
const BLUR_SCALE: u32 = 16;
/// Small blurred copy of a wallpaper frame (straight RGBA, top row first):
/// every 16x16 block averaged, then two 3x3 box passes. Scaled back up with
/// smooth filtering it reads as a heavy blur, for a fraction of the cost.
pub fn blurred(pixels: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = (width / BLUR_SCALE, height / BLUR_SCALE);
    if w == 0 || h == 0 || pixels.len() < (width * height * 4) as usize {
        return None;
    }
    let mut small = vec![255u8; (w * h * 4) as usize];
    let count = BLUR_SCALE * BLUR_SCALE;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            for dy in 0..BLUR_SCALE {
                let row = ((y * BLUR_SCALE + dy) * width + x * BLUR_SCALE) as usize * 4;
                for px in pixels[row..row + (BLUR_SCALE * 4) as usize].chunks_exact(4) {
                    acc[0] += u32::from(px[0]);
                    acc[1] += u32::from(px[1]);
                    acc[2] += u32::from(px[2]);
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                small[o + c] = (acc[c] / count) as u8;
            }
        }
    }
    for _ in 0..2 {
        small = box3(&small, w, h);
    }
    Some((small, w, h))
}
fn box3(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![255u8; src.len()];
    let (w, h) = (w as i64, h as i64);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            let mut n = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= w || sy >= h {
                        continue;
                    }
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..3 {
                        acc[c] += u32::from(src[i + c]);
                    }
                    n += 1;
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                out[o + c] = (acc[c] / n) as u8;
            }
        }
    }
    out
}
#[cfg(test)]
mod blur_tests {
    use super::*;
    #[test]
    fn backdrop_is_downsampled_and_smoothed() {
        let (w, h) = (64u32, 48u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        // Left half white, right half black, opaque.
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let v = if x < w / 2 { 255 } else { 0 };
                pixels[i..i + 3].copy_from_slice(&[v, v, v]);
                pixels[i + 3] = 255;
            }
        }
        let (small, sw, sh) = blurred(&pixels, w, h).unwrap();
        assert_eq!((sw, sh), (4, 3));
        assert_eq!(small.len(), 4 * 3 * 4);
        let px = |x: usize| small[x * 4];
        assert!(
            px(0) > px(1) && px(1) > px(2) && px(2) > px(3),
            "edge is smoothed"
        );
        assert!(small.chunks_exact(4).all(|p| p[3] == 255));
        assert!(blurred(&pixels, 8, 8).is_none());
    }
}
