//! Best-effort, lossless thumbnail cache, outside the watched configuration.
//! Raw RGBA trades disk space for cheap reads (no PNG encode/decode on reopen).
use super::render;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use winarchy_theme::preview::Entry;

const WIDTH: u32 = 1536;
const HEIGHT: u32 = 864;
const PIXELS: usize = WIDTH as usize * HEIGHT as usize * 4;
const LIMIT: u64 = 256 * 1024 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn root() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")));
    // Bump when thumbnail dimensions, crop or filtering semantics change.
    base.map(|p| p.join("winarchy/picker-thumbnails-v1"))
}
fn identity(entry: &Entry) -> Vec<u8> {
    let mut key = b"winarchy-thumbnail-v1\0".to_vec();
    key.extend_from_slice(&entry.size.to_le_bytes());
    key.extend_from_slice(&entry.modified.to_le_bytes());
    key.extend_from_slice(entry.path.as_os_str().as_encoded_bytes());
    key
}
fn filename(key: &[u8]) -> String {
    // Stable FNV-1a filename; the full identity is also checked on read, so
    // hash collisions are misses, never another image's pixels.
    let hash = key.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}.rgba")
}
fn load(path: &Path, key: &[u8]) -> Option<image::RgbaImage> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() != (key.len() + PIXELS) as u64 {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let mut header = vec![0; key.len()];
    file.read_exact(&mut header).ok()?;
    if header != key {
        return None;
    }
    let mut pixels = vec![0; PIXELS];
    file.read_exact(&mut pixels).ok()?;
    image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels)
}
fn prune(root: &Path) {
    let Ok(files) = fs::read_dir(root) else {
        return;
    };
    let mut rows: Vec<_> = files
        .filter_map(Result::ok)
        .filter_map(|f| {
            let path = f.path();
            if path.extension()? != "rgba" {
                return None;
            }
            let meta = fs::symlink_metadata(&path).ok()?;
            meta.file_type()
                .is_file()
                .then(|| (meta.modified().ok(), path, meta.len()))
        })
        .collect();
    let mut bytes: u64 = rows.iter().map(|r| r.2).sum();
    rows.sort_by_key(|r| r.0);
    for (_, path, size) in rows {
        if bytes <= LIMIT {
            break;
        }
        if fs::remove_file(path).is_ok() {
            bytes -= size;
        }
    }
}
fn save(root: &Path, path: &Path, key: &[u8], image: &image::RgbaImage) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let temp = root.join(format!(
        "{}.{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(key)?;
        file.write_all(image.as_raw())?;
        drop(file);
        // Windows rename cannot replace a stale/corrupt destination. Readers
        // treat the short missing-file interval as a cache miss.
        let _ = fs::remove_file(path);
        fs::rename(&temp, path)
    })();
    let _ = fs::remove_file(&temp);
    result
}
fn thumbnail_at(entry: &Entry, root: Option<&Path>) -> Result<image::RgbaImage, String> {
    let key = identity(entry);
    let path = root.map(|p| p.join(filename(&key)));
    if let Some(image) = path.as_deref().and_then(|p| load(p, &key)) {
        return Ok(image);
    }
    let image = render::thumbnail(entry)?;
    if let (Some(root), Some(path)) = (root, path) {
        if save(root, &path, &key, &image).is_ok() {
            prune(root);
        }
    }
    Ok(image)
}
pub fn thumbnail(entry: &Entry) -> Result<image::RgbaImage, String> {
    thumbnail_at(entry, root().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_hit_invalidation_corruption_and_unwritable_cache() {
        let dir = std::env::temp_dir().join(format!("picker-disk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.png");
        image::RgbaImage::from_pixel(16, 9, image::Rgba([21, 42, 84, 128]))
            .save(&source)
            .unwrap();
        let mut entry = Entry {
            id: "test".into(),
            path: source.clone(),
            size: 1,
            modified: 1,
        };
        let cache = dir.join("cache");
        let first = thumbnail_at(&entry, Some(&cache)).unwrap();
        // A hit must not decode the original. Catalog fingerprints are the
        // caller's responsibility and change when the real source changes.
        fs::write(&source, "broken").unwrap();
        assert_eq!(thumbnail_at(&entry, Some(&cache)).unwrap(), first);
        entry.modified += 1;
        assert!(thumbnail_at(&entry, Some(&cache)).is_err());
        entry.modified -= 1;
        fs::write(cache.join(filename(&identity(&entry))), "truncated").unwrap();
        assert!(thumbnail_at(&entry, Some(&cache)).is_err());
        image::RgbaImage::new(16, 9).save(&source).unwrap();
        assert!(
            thumbnail_at(&entry, Some(&source)).is_ok(),
            "cache failure is nonfatal"
        );
        assert!(
            thumbnail_at(&entry, Some(&cache)).is_ok(),
            "corrupt cache is repaired"
        );
        assert!(load(&cache.join(filename(&identity(&entry))), b"wrong identity").is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
