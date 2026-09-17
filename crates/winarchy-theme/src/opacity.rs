//! Session-only override shared by application processes. The daemon clears it
//! on startup, shutdown and theme switches; installed theme files are untouched.
use crate::{Theme, read};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "background-opacity.state";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Override {
    theme: String,
    opacity: f32,
}

/// Five percentage points, bounded interactively to keep windows visible.
pub fn step(current: f32, increase: bool) -> f32 {
    let percent = (current * 100.0).round() as i32;
    (percent + if increase { 5 } else { -5 }).clamp(5, 100) as f32 / 100.0
}

pub fn apply(home: &Path, name: &str, theme: &mut Theme) {
    if let Ok(text) = read(&home.join(FILE))
        && let Ok(value) = toml::from_str::<Override>(&text)
        && value.theme == name
        && value.opacity.is_finite()
        && (0.05..=1.0).contains(&value.opacity)
    {
        theme.background_opacity = value.opacity;
    }
}

pub fn set(home: &Path, name: &str, opacity: f32) -> Result<(), String> {
    if !crate::valid_name(name) || !opacity.is_finite() || !(0.05..=1.0).contains(&opacity) {
        return Err("invalid background opacity override".into());
    }
    let text = toml::to_string(&Override {
        theme: name.into(),
        opacity,
    })
    .map_err(|e| e.to_string())?;
    // Same-directory rename replaces the old snapshot atomically on Windows too.
    let temporary = home.join(format!("{FILE}.tmp"));
    std::fs::write(&temporary, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, home.join(FILE)).map_err(|e| e.to_string())
}

pub fn clear(home: &Path) -> Result<(), String> {
    match std::fs::remove_file(home.join(FILE)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn increments_and_bounds() {
        assert_eq!(step(0.85, false), 0.80);
        assert_eq!(step(0.85, true), 0.90);
        assert_eq!(step(0.83, false), 0.78);
        assert_eq!(step(0.05, false), 0.05);
        assert_eq!(step(0.0, false), 0.05);
        assert_eq!(step(1.0, true), 1.0);
        let mut opacity = 0.85;
        for _ in 0..3 {
            opacity = step(opacity, true);
        }
        assert_eq!(opacity, 1.0);
    }
    #[test]
    fn shared_override_is_atomic_scoped_and_resettable() {
        let home = crate::tests::home();
        let mut theme = Theme::default_theme();
        set(&home, "mine", 0.65).unwrap();
        set(&home, "mine", 0.70).unwrap();
        apply(&home, "other", &mut theme);
        assert_eq!(theme.background_opacity, 0.85);
        apply(&home, "mine", &mut theme);
        assert_eq!(theme.background_opacity, 0.70);
        assert!(set(&home, "mine", f32::NAN).is_err());
        assert!(set(&home, "mine", 0.0).is_err());
        clear(&home).unwrap();
        clear(&home).unwrap();
        theme = Theme::default_theme();
        apply(&home, "mine", &mut theme);
        assert_eq!(theme.background_opacity, 0.85);
        for invalid in [
            "broken",
            "theme = 'mine'\nopacity = nan",
            "theme = 'mine'\nopacity = 2.0",
        ] {
            std::fs::write(home.join(FILE), invalid).unwrap();
            apply(&home, "mine", &mut theme);
            assert_eq!(theme.background_opacity, 0.85);
        }
        std::fs::remove_dir_all(home).unwrap();
    }
}
