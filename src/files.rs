//! Bounded reads and discovery. User configuration is trusted, but not unbounded.
use std::{
    io::Read,
    path::{Path, PathBuf},
};
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const MAX_CONFIG_ENTRIES: usize = 256;
fn reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
pub fn read_config(path: &Path) -> Result<Vec<u8>, String> {
    read_bounded(path, MAX_CONFIG_BYTES)
}
pub fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let result = (|| -> Result<Vec<u8>, String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || reparse(&metadata) {
            return Err("expected a regular non-reparse file".into());
        }
        if metadata.len() > max as u64 {
            return Err(format!("file exceeds {} KiB", max / 1024));
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take((max + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > max {
            return Err(format!("file exceeds {} KiB", max / 1024));
        }
        Ok(bytes)
    })();
    result.map_err(|e| format!("{}: {e}", path.display()))
}
pub fn snapshot(home: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut result = Vec::new();
    let mut examined = 0;
    for dir in [home.to_path_buf(), home.join("themes")] {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            examined += 1;
            if examined > MAX_CONFIG_ENTRIES {
                return Err("configuration directory exceeds 256 entries".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|s| s == "toml") {
                result.push((path.clone(), read_config(&path)?));
            }
        }
    }
    // Applet folders: any file counts (manifest, view, script, icon), by
    // fingerprint rather than content, so edits reload without rereading.
    if let Ok(applets) = std::fs::read_dir(home.join("applets")) {
        for applet in applets.flatten().take(64) {
            let Ok(files) = std::fs::read_dir(applet.path()) else {
                continue;
            };
            for file in files.flatten().take(32) {
                let Ok(meta) = file.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                result.push((
                    file.path(),
                    format!("{}:{modified}", meta.len()).into_bytes(),
                ));
            }
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
/// Iterative traversal: never recurse through links/junctions, and stop at limits.
pub fn shortcuts(root: &Path, max_entries: usize, max_depth: usize) -> Vec<PathBuf> {
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut result = Vec::new();
    let mut examined = 0;
    while let Some((dir, depth)) = pending.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&dir) else {
            continue;
        };
        if reparse(&meta) || !meta.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if examined == max_entries {
                result.sort();
                return result;
            }
            examined += 1;
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if reparse(&meta) {
                continue;
            }
            if meta.is_dir() && depth < max_depth {
                pending.push((path, depth + 1));
            } else if meta.is_file()
                && path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("lnk"))
            {
                result.push(path);
            }
        }
    }
    result.sort();
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new(name: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("winarchy-files-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn bounded_reads() {
        let t = Temp::new("reads");
        let p = t.0.join("x.toml");
        std::fs::write(&p, vec![b'a'; MAX_CONFIG_BYTES]).unwrap();
        assert_eq!(read_config(&p).unwrap().len(), MAX_CONFIG_BYTES);
        std::fs::write(&p, vec![b'a'; MAX_CONFIG_BYTES + 1]).unwrap();
        assert!(read_config(&p).is_err());
        assert!(read_config(&t.0).is_err());
    }
    #[test]
    fn bounded_discovery() {
        let t = Temp::new("walk");
        std::fs::create_dir(t.0.join("child")).unwrap();
        for i in 0..10 {
            std::fs::write(t.0.join(format!("{i}.lnk")), "").unwrap();
        }
        std::fs::write(t.0.join("child/deep.lnk"), "").unwrap();
        assert_eq!(shortcuts(&t.0, 100, 0).len(), 10);
        assert_eq!(shortcuts(&t.0, 100, 1).len(), 11);
        assert!(shortcuts(&t.0, 3, 16).len() <= 3);
    }
    #[cfg(unix)]
    #[test]
    fn ignores_link_cycles_and_linked_files() {
        let t = Temp::new("links");
        std::os::unix::fs::symlink(&t.0, t.0.join("cycle")).unwrap();
        let file = t.0.join("x.toml");
        std::fs::write(&file, "ok").unwrap();
        std::os::unix::fs::symlink(&file, t.0.join("link.toml")).unwrap();
        assert!(read_config(&t.0.join("link.toml")).is_err());
        assert!(shortcuts(&t.0, 100, 16).is_empty());
    }
}
