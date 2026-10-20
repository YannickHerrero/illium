//! Launch counts of the launcher entries, so often used applications rank
//! above alphabetical neighbours with the same match quality.
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
const MAX_BYTES: usize = 256 * 1024;
#[derive(Debug, Default)]
pub struct History {
    /// Lowercased application name to launch count. Names survive a reinstall
    /// or a version change in the shortcut target, paths do not.
    counts: BTreeMap<String, u32>,
    path: Option<PathBuf>,
}
impl History {
    /// A missing or unreadable file is not an error: every count starts at zero.
    pub fn load(path: &Path) -> Self {
        let counts = crate::files::read_bounded(path, MAX_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            counts,
            path: Some(path.to_path_buf()),
        }
    }
    pub fn count(&self, name: &str) -> u32 {
        self.counts.get(&name.to_lowercase()).copied().unwrap_or(0)
    }
    pub fn record(&mut self, name: &str) -> Result<(), String> {
        let count = self.counts.entry(name.to_lowercase()).or_insert(0);
        *count = count.saturating_add(1);
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_string(&self.counts).map_err(|e| e.to_string())?;
        crate::state::State::save(&json, path)
    }
}
/// Most launched first, then best match, then name.
pub fn compare(a: (u32, usize, &str), b: (u32, usize, &str)) -> Ordering {
    b.0.cmp(&a.0)
        .then_with(|| a.1.cmp(&b.1))
        .then_with(|| a.2.cmp(b.2))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_count_outranks_match_score_and_name() {
        let mut items = vec![
            (0, 3, "Power Automate"),
            (1, 3, "PowerShell"),
            (0, 0, "Pow"),
            (5, 9, "Visual Studio Code"),
        ];
        items.sort_by(|a, b| compare(*a, *b));
        let names: Vec<_> = items.iter().map(|i| i.2).collect();
        assert_eq!(
            names,
            ["Visual Studio Code", "PowerShell", "Pow", "Power Automate"]
        );
    }
    #[test]
    fn counts_persist_across_loads_and_ignore_case() {
        let dir = std::env::temp_dir().join(format!("illium-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launcher-history.json");
        let _ = std::fs::remove_file(&path);
        let mut history = History::load(&path);
        assert_eq!(history.count("PowerShell"), 0);
        history.record("PowerShell").unwrap();
        history.record("powershell").unwrap();
        let reloaded = History::load(&path);
        assert_eq!(reloaded.count("POWERSHELL"), 2);
        assert_eq!(reloaded.count("Power Automate"), 0);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(History::load(&path).count("powershell"), 0);
    }
}
