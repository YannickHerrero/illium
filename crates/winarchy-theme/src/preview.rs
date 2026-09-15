//! Bounded, data-only discovery of theme preview images. No decoding on scan.
use crate::pack;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_ENTRIES: usize = 1024;
const MAX_THEMES: usize = 256;
const NAMES: [&str; 3] = ["preview.png", "preview.jpg", "preview.jpeg"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: u128,
}

/// Case-insensitive names, deterministic priority (PNG, JPG, JPEG).
/// Reject rather than follow a preview link, including during pack installation.
pub fn files(dir: &Path) -> Result<Vec<String>, String> {
    if !dir.try_exists().map_err(|e| e.to_string())? {
        return Ok(vec![]);
    }
    pack::metadata(dir, true)?;
    let mut names = vec![];
    for (n, entry) in fs::read_dir(dir).map_err(|e| e.to_string())?.enumerate() {
        if n >= MAX_ENTRIES {
            return Err("preview directory exceeds 1024 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(rank) = NAMES.iter().position(|p| name.eq_ignore_ascii_case(p)) {
            pack::metadata(&entry.path(), false)?;
            names.push((rank, name));
        }
    }
    names.sort();
    Ok(names.into_iter().map(|(_, name)| name).collect())
}

fn entry(home: &Path, id: String) -> Result<Option<Entry>, String> {
    let themes = home.join("themes");
    let palette = pack::read(&themes.join(format!("{id}.toml")), 65536)?;
    crate::Theme::parse(std::str::from_utf8(&palette).map_err(|e| e.to_string())?)?;
    // wallpaper_dir also validates every parent directory, even without wallpapers.
    let wallpaper_dir = pack::wallpaper_dir(home, &id)?;
    let assets = themes.join(&id);
    let path = if let Some(name) = files(&assets)?.first() {
        assets.join(name)
    } else if let Some(name) = pack::images(&wallpaper_dir)?.first() {
        wallpaper_dir.join(name)
    } else {
        return Ok(None);
    };
    let meta = pack::metadata(&path, false)?;
    if meta.len() > pack::MAX_IMAGE_BYTES as u64 {
        return Err("preview image too large".into());
    }
    let modified = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(Some(Entry {
        id,
        path,
        size: meta.len(),
        modified,
    }))
}

/// Invalid individual themes/assets are omitted; unrelated themes remain usable.
/// Read on a worker, never from a keyboard callback. Equality is also the watcher
/// fingerprint: file edits in place invalidate a preview without decoding it.
pub fn catalog(home: &Path) -> Result<Vec<Entry>, String> {
    let themes = home.join("themes");
    pack::metadata(&themes, true)?;
    let mut ids = vec![];
    for (n, item) in fs::read_dir(&themes)
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        if n >= MAX_ENTRIES {
            return Err("themes directory exceeds 1024 entries".into());
        }
        let path = item.map_err(|e| e.to_string())?.path();
        if !path.extension().is_some_and(|s| s == "toml") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if crate::valid_name(id) && pack::plain_name(id) {
            ids.push(id.to_owned());
        }
    }
    if ids.len() > MAX_THEMES {
        return Err("theme catalog exceeds 256 themes".into());
    }
    ids.sort();
    Ok(ids
        .into_iter()
        .filter_map(|id| entry(home, id).ok().flatten())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "winarchy-preview-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("source/mine/wallpapers")).unwrap();
            fs::write(path.join("source/mine/theme.toml"), crate::DEFAULT).unwrap();
            Self(path)
        }
        fn source(&self) -> PathBuf {
            self.0.join("source/mine")
        }
        fn home(&self) -> PathBuf {
            self.0.join("home")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn png(path: &Path) {
        image::RgbaImage::new(2, 2).save(path).unwrap();
    }
    #[test]
    fn install_preview_with_priority_and_fallback() {
        let t = Temp::new();
        png(&t.source().join("PREVIEW.PNG"));
        image::RgbImage::new(2, 2)
            .save(t.source().join("preview.jpg"))
            .unwrap();
        png(&t.source().join("wallpapers/b.png"));
        png(&t.source().join("wallpapers/a.png"));
        pack::install(&t.home(), &t.source()).unwrap();
        let rows = catalog(&t.home()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "mine");
        assert!(rows[0].path.ends_with("PREVIEW.PNG"));
        fs::remove_file(&rows[0].path).unwrap();
        assert!(catalog(&t.home()).unwrap()[0].path.ends_with("preview.jpg"));
        fs::remove_file(t.home().join("themes/mine/preview.jpg")).unwrap();
        assert!(
            catalog(&t.home()).unwrap()[0]
                .path
                .ends_with("wallpapers/a.png")
        );
        assert!(t.source().join("PREVIEW.PNG").is_file());
    }
    #[test]
    fn no_assets_no_placeholder_and_bad_preview_rolls_back() {
        let t = Temp::new();
        fs::write(t.source().join("preview.png"), "broken").unwrap();
        assert!(pack::install(&t.home(), &t.source()).is_err());
        assert_eq!(fs::read_dir(t.home().join("themes")).unwrap().count(), 0);
        fs::remove_file(t.source().join("preview.png")).unwrap();
        pack::install(&t.home(), &t.source()).unwrap();
        assert!(catalog(&t.home()).unwrap().is_empty());
        png(&t.home().join("themes/mine/preview.png"));
        assert_eq!(catalog(&t.home()).unwrap().len(), 1);
    }
    #[test]
    fn fingerprints_detect_in_place_edits_and_invalid_palettes() {
        let t = Temp::new();
        png(&t.source().join("preview.png"));
        pack::install(&t.home(), &t.source()).unwrap();
        let before = catalog(&t.home()).unwrap();
        fs::write(&before[0].path, "changed size").unwrap();
        assert_ne!(before, catalog(&t.home()).unwrap());
        fs::write(t.home().join("themes/mine.toml"), "broken").unwrap();
        assert!(catalog(&t.home()).unwrap().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn preview_links_are_not_installed_or_discovered() {
        let t = Temp::new();
        png(&t.0.join("outside.png"));
        std::os::unix::fs::symlink(t.0.join("outside.png"), t.source().join("preview.png"))
            .unwrap();
        assert!(pack::install(&t.home(), &t.source()).is_err());
        fs::remove_file(t.source().join("preview.png")).unwrap();
        pack::install(&t.home(), &t.source()).unwrap();
        std::os::unix::fs::symlink(
            t.0.join("outside.png"),
            t.home().join("themes/mine/preview.png"),
        )
        .unwrap();
        assert!(catalog(&t.home()).unwrap().is_empty());
    }
}
