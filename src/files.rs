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
/// Create one component beneath an already checked directory. Existing links
/// and junctions are not valid asset/default directories.
pub(crate) fn create_directory(path: &Path) -> Result<(), String> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || reparse(&meta) {
        return Err(format!(
            "{}: expected a regular non-reparse directory",
            path.display()
        ));
    }
    Ok(())
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
/// Palette/terminal-only edits must not rebuild the shell or restart applet providers.
pub fn same_subsystems(
    home: &Path,
    before: &[(PathBuf, Vec<u8>)],
    after: &[(PathBuf, Vec<u8>)],
) -> bool {
    let global = home.join("winarchy.toml");
    let themes = home.join("themes");
    let terminal = home.join("terminal.toml");
    let subsystem = |entry: &&(PathBuf, Vec<u8>)| {
        entry.0 != global && entry.0 != terminal && entry.0.parent() != Some(themes.as_path())
    };
    before
        .iter()
        .filter(subsystem)
        .eq(after.iter().filter(subsystem))
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
            if applet.file_type().is_ok_and(|kind| kind.is_dir()) {
                result.extend(applet_snapshot(&applet.path())?);
            }
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
/// Fingerprint installed applet sources, including imported views/connectors.
/// Incomplete or linked trees cannot be used to validate compiled definitions.
pub fn applet_snapshot(dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut pending = vec![(dir.to_owned(), 0)];
    let mut files = Vec::new();
    let mut examined = 0;
    while let Some((path, depth)) = pending.pop() {
        examined += 1;
        if examined > 512 || depth > 16 {
            return Err(format!(
                "{}: applet source tree exceeds limits",
                dir.display()
            ));
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if reparse(&meta) {
            return Err(format!("{}: linked applet source refused", path.display()));
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).map_err(|e| e.to_string())? {
                if pending.len() + examined >= 512 {
                    return Err(format!(
                        "{}: applet source tree exceeds limits",
                        dir.display()
                    ));
                }
                pending.push((entry.map_err(|e| e.to_string())?.path(), depth + 1));
            }
        } else if meta.is_file() {
            let modified = meta
                .modified()
                .map_err(|e| e.to_string())?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            files.push((path, format!("{}:{modified}", meta.len()).into_bytes()));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
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
    fn applet_fingerprints_include_nested_changes_and_refuse_partial_trees() {
        let t = Temp::new("applet-sources");
        std::fs::create_dir_all(t.0.join("connectors")).unwrap();
        let file = t.0.join("connectors/provider.ps1");
        std::fs::write(&file, "before").unwrap();
        let before = applet_snapshot(&t.0).unwrap();
        assert_eq!(before, applet_snapshot(&t.0).unwrap());
        std::fs::write(&file, "after edit").unwrap();
        assert_ne!(before, applet_snapshot(&t.0).unwrap());
        std::fs::remove_file(file).unwrap();
        assert!(applet_snapshot(&t.0).unwrap().is_empty());
        for i in 0..512 {
            std::fs::write(t.0.join(format!("{i}.slint")), "").unwrap();
        }
        assert!(applet_snapshot(&t.0).is_err());
    }
    #[test]
    fn temporary_opacity_does_not_reload_shell_configuration() {
        let t = Temp::new("opacity");
        std::fs::create_dir(t.0.join("themes")).unwrap();
        std::fs::write(t.0.join("winarchy.toml"), "theme = 'test'").unwrap();
        let before = snapshot(&t.0).unwrap();
        winarchy_theme::opacity::set(&t.0, "test", 0.65).unwrap();
        assert_eq!(snapshot(&t.0).unwrap(), before);
        winarchy_theme::opacity::clear(&t.0).unwrap();
        assert_eq!(snapshot(&t.0).unwrap(), before);
    }
    #[test]
    fn terminal_preferences_do_not_restart_shell_subsystems() {
        let home = Path::new("home");
        let before = vec![(home.join("terminal.toml"), b"font_size=14".to_vec())];
        let after = vec![(home.join("terminal.toml"), b"font_size=18".to_vec())];
        assert!(same_subsystems(home, &before, &after));
    }
    #[test]
    fn recognizes_palette_only_edits_but_not_applet_changes() {
        let home = Path::new("home");
        let before = vec![
            (home.join("applets/test/script.ps1"), vec![1]),
            (home.join("themes/dark.toml"), vec![2]),
            (home.join("winarchy.toml"), vec![3]),
            (home.join("wm.toml"), vec![4]),
        ];
        let mut after = before.clone();
        after[1].1 = vec![5];
        after[2].1 = vec![6];
        assert!(same_subsystems(home, &before, &after));
        after[0].1 = vec![7];
        assert!(!same_subsystems(home, &before, &after));
        after = before.clone();
        after[3].1 = vec![8];
        assert!(!same_subsystems(home, &before, &after));
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
        assert!(applet_snapshot(&t.0).is_err());
    }
}
