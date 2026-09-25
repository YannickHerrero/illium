//! Best-effort persistent palette cache, never a prerequisite for displaying an
//! image. Only this worker-owned directory is pruned, at most 64 small entries.
use super::{Snapshot, VERSION, generate, state::atomic_write};
use crate::pack;
use sha2::{Digest, Sha256};
use std::path::Path;

fn directory(home: &Path) -> Result<std::path::PathBuf, String> {
    let mut path = home.to_path_buf();
    for component in ["cache", "dynamic-palettes"] {
        path.push(component);
        match std::fs::create_dir(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.to_string()),
        }
        pack::metadata(&path, true)?;
    }
    Ok(path)
}
fn save(dir: &Path, filename: &str, value: &Snapshot) -> Result<(), String> {
    let mut entries = Vec::new();
    for (n, entry) in std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        if n >= 128 {
            return Err("dynamic cache directory exceeds limits".into());
        }
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|s| s == "palette")
            && stem.len() == 64
            && stem.bytes().all(|b| b.is_ascii_hexdigit())
        {
            let meta = pack::metadata(&path, false)?;
            entries.push((meta.modified().map_err(|e| e.to_string())?, path));
        }
    }
    entries.sort();
    let remove = entries.len().saturating_sub(63);
    for (_, path) in entries.into_iter().take(remove) {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    atomic_write(
        &dir.join(filename),
        &toml::to_string(value).map_err(|e| e.to_string())?,
    )
}
/// Source pixels have already passed the wallpaper decoder's size limits. Hash
/// decoded content and dimensions, not mtime, path, screen crop or mode.
pub fn prepare(home: &Path, source: &str, pixels: &image::RgbaImage) -> Snapshot {
    let mut hash = Sha256::new();
    hash.update(VERSION.to_le_bytes());
    hash.update(pixels.width().to_le_bytes());
    hash.update(pixels.height().to_le_bytes());
    hash.update(pixels.as_raw());
    let hash = format!("{:x}", hash.finalize());
    let filename = format!("{hash}.palette");
    let dir = directory(home).ok();
    if let Some(dir) = &dir
        && let Ok(bytes) = pack::read(&dir.join(&filename), 65536)
        && let Ok(text) = std::str::from_utf8(&bytes)
        && let Ok(mut value) = toml::from_str::<Snapshot>(text)
        && value.hash == hash
        && value.validate().is_ok()
    {
        value.source = source.into();
        return value;
    }
    let value = Snapshot {
        version: VERSION,
        source: source.into(),
        hash,
        palettes: generate(pixels),
    };
    if let Some(dir) = dir {
        let _ = save(&dir, &filename, &value);
    }
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reuse_invalidation_corruption_and_bounded_cache() {
        let home = crate::tests::home();
        let mut pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 200, 255]));
        let first = prepare(&home, "a.png", &pixels);
        let mut renamed = prepare(&home, "b.png", &pixels);
        renamed.source = first.source.clone();
        assert_eq!(first, renamed);
        let dir = directory(&home).unwrap();
        std::fs::write(dir.join(format!("{}.palette", first.hash)), "broken").unwrap();
        assert_eq!(prepare(&home, "a.png", &pixels), first);
        for r in 0..70 {
            pixels.put_pixel(0, 0, image::Rgba([r, 20, 200, 255]));
            let changed = prepare(&home, "a.png", &pixels);
            if r != 10 {
                assert_ne!(changed.hash, first.hash);
            }
        }
        assert!(std::fs::read_dir(dir).unwrap().count() <= 64);
        std::fs::remove_dir_all(home).unwrap();
    }
}
