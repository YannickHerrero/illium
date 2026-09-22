use super::{FILE, Palettes, VERSION, is_dynamic};
use crate::Theme;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub source: String,
    pub hash: String,
    pub palettes: Palettes,
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || self.source.is_empty()
            || self.source.contains(['/', '\\', ':'])
            || self.hash.len() != 64
            || !self.hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid dynamic palette snapshot".into());
        }
        self.palettes.validate()
    }
    pub fn apply(&self, name: &str, theme: &mut Theme) {
        if is_dynamic(name) {
            let opacity = theme.background_opacity;
            *theme = self.palettes.get(name).clone();
            theme.background_opacity = opacity;
        }
    }
}
/// Invalid runtime edits are errors, so live subscribers retain their last valid
/// palette. Missing or old-version snapshots use the installed fallback.
pub fn apply(home: &Path, name: &str, theme: &mut Theme) -> Result<(), String> {
    if !is_dynamic(name) {
        return Ok(());
    }
    let path = home.join(FILE);
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let value: Snapshot = toml::from_str(&crate::read(&path)?).map_err(|e| e.to_string())?;
    if value.version != VERSION {
        return Ok(());
    }
    value.validate()?;
    value.apply(name, theme);
    Ok(())
}
pub(super) fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result
}
pub fn publish(home: &Path, snapshot: Option<&Snapshot>) -> Result<(), String> {
    let Some(value) = snapshot else {
        return match std::fs::remove_file(home.join(FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    };
    value.validate()?;
    let text = toml::to_string(value).map_err(|e| e.to_string())?;
    atomic_write(&home.join(FILE), &text)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            version: VERSION,
            source: "forest.png".into(),
            hash: "a".repeat(64),
            palettes: Palettes {
                dark: Theme::parse(include_str!("../../../../config/themes/dynamic-dark.toml"))
                    .unwrap(),
                light: Theme::parse(include_str!("../../../../config/themes/dynamic-light.toml"))
                    .unwrap(),
            },
        }
    }
    #[test]
    fn publication_scopes_variants_preserves_opacity_and_rejects_corruption() {
        let home = crate::tests::home();
        let value = snapshot();
        publish(&home, Some(&value)).unwrap();
        publish(&home, Some(&value)).unwrap();
        let mut theme = Theme::default_theme();
        apply(&home, "static", &mut theme).unwrap();
        assert_eq!(theme, Theme::default_theme());
        theme.background_opacity = 0.6;
        apply(&home, "dynamic-light", &mut theme).unwrap();
        assert_eq!(theme.mode.as_deref(), Some("light"));
        assert_eq!(theme.background_opacity, 0.6);
        std::fs::write(home.join(FILE), "broken").unwrap();
        assert!(apply(&home, "dynamic-dark", &mut theme).is_err());
        publish(&home, None).unwrap();
        publish(&home, None).unwrap();
        assert!(apply(&home, "dynamic-dark", &mut theme).is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }
}
