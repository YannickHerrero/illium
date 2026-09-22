//! Bounded local history and bookmarks. No network suggestions or background indexer.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

const HISTORY_LIMIT: usize = 500;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
    pub url: String,
    pub title: String,
    /// Unix seconds; absent in libraries written before the navigation palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visited_at: Option<u64>,
}
#[derive(Default, Serialize, Deserialize)]
struct Data {
    history: Vec<Site>, // newest first
    bookmarks: Vec<Site>,
}
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub site: Site,
    pub bookmarked: bool,
}
impl Suggestion {
    pub fn description(&self, now: u64) -> String {
        let Some(visited_at) = self.site.visited_at.filter(|_| !self.bookmarked) else {
            return self.site.title.clone();
        };
        let age = now.saturating_sub(visited_at);
        let relative = match age {
            0..60 => "Visited just now".to_owned(),
            60..3600 => format!("Visited {}m ago", age / 60),
            3600..86400 => format!("Visited {}h ago", age / 3600),
            86400..172800 => "Visited 1 day ago".to_owned(),
            _ => format!("Visited {} days ago", age / 86400),
        };
        if self.site.title.is_empty() {
            relative
        } else {
            format!("{} · {relative}", self.site.title)
        }
    }
}
pub struct Library {
    dir: PathBuf,
    data: Data,
    stamp: Option<(u64, SystemTime)>,
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn site(url: &str, title: &str) -> Option<Site> {
    let mut url = Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    // Never persist embedded login credentials.
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Some(Site {
        url: url.into(),
        visited_at: None,
        title: title
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
    })
}
fn stamp(dir: &Path) -> Result<Option<(u64, SystemTime)>> {
    match fs::metadata(dir.join("library.json")) {
        Ok(meta) => Ok(Some((meta.len(), meta.modified()?))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
impl Library {
    pub fn load(dir: &Path) -> Result<Self> {
        // Observe before reading: a concurrent atomic replacement can at worst
        // cause an extra reload, never mark an older snapshot as the new file.
        let stamp = stamp(dir)?;
        let data = match fs::read(dir.join("library.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            dir: dir.into(),
            data,
            stamp,
        })
    }
    /// A small file lock and reload avoid losing another browser window's edits.
    fn update<T>(&mut self, change: impl FnOnce(&mut Data) -> T) -> Result<T> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join("library.lock"))?;
        lock.lock()?;
        let mut latest = Self::load(&self.dir)?.data;
        let result = change(&mut latest);
        let temp = self.dir.join("library.json.tmp");
        fs::write(&temp, serde_json::to_vec(&latest)?)?;
        fs::rename(temp, self.dir.join("library.json"))?;
        self.data = latest;
        self.stamp = stamp(&self.dir)?;
        Ok(result)
    }
    pub fn reload(&mut self) -> Result<()> {
        if stamp(&self.dir)? != self.stamp {
            *self = Self::load(&self.dir)?;
        }
        Ok(())
    }
    pub fn visit(&mut self, url: &str, title: &str) -> Result<()> {
        let Some(mut site) = site(url, title) else {
            return Ok(());
        };
        site.visited_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
        self.update(|data| {
            data.history.retain(|s| s.url != site.url);
            data.history.insert(0, site);
            data.history.truncate(HISTORY_LIMIT);
        })
    }
    /// Add the current page once. Repeating Ctrl+D never removes a bookmark.
    pub fn add_bookmark(&mut self, url: &str, title: &str) -> Result<Option<bool>> {
        let Some(site) = site(url, title) else {
            return Ok(None);
        };
        self.update(|data| {
            if let Some(i) = data.bookmarks.iter().position(|s| s.url == site.url) {
                data.bookmarks[i] = site;
                Some(false)
            } else {
                data.bookmarks.insert(0, site);
                Some(true)
            }
        })
    }
    pub fn suggestions(&self, query: &str) -> Vec<Suggestion> {
        let mut seen = std::collections::HashSet::new();
        let mut matches = Vec::new();
        for (bookmarked, sites) in [(true, &self.data.bookmarks), (false, &self.data.history)] {
            for (recency, site) in sites.iter().enumerate() {
                if !seen.insert(&site.url) {
                    continue;
                }
                if let Some(score) = fuzzy(query, &format!("{} {}", site.title, site.url)) {
                    matches.push((
                        score,
                        !bookmarked,
                        recency,
                        Suggestion {
                            site: site.clone(),
                            bookmarked,
                        },
                    ));
                }
            }
        }
        matches.sort_by_key(|(score, history, recency, _)| (*score, *history, *recency));
        matches
            .into_iter()
            .take(8)
            .map(|(_, _, _, suggestion)| suggestion)
            .collect()
    }
}
/// Case-insensitive ordered subsequence. Prefer contiguous matches, then early ones.
pub(crate) fn fuzzy(query: &str, text: &str) -> Option<usize> {
    let query: Vec<_> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<_> = text.to_lowercase().chars().collect();
    let mut next = 0;
    let mut previous = None;
    let mut score = 0;
    for needle in query {
        let index = (next..text.len()).find(|&i| text[i] == needle)?;
        score += match previous {
            Some(p) => (index - p - 1) * 10,
            None => index,
        };
        previous = Some(index);
        next = index + 1;
    }
    Some(score)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fuzzy_subsequence() {
        assert!(fuzzy("GHB", "GitHub https://github.com").is_some());
        assert!(fuzzy("xyz", "GitHub").is_none());
        assert!(fuzzy("gh", "gh") < fuzzy("gh", "g---h"));
        assert!(fuzzy("é", "CAFÉ").is_some());
    }
    #[test]
    fn legacy_library_and_relative_dates() {
        let data: Data = serde_json::from_str(
            r#"{"history":[{"url":"https://example.com/","title":"Example"}],"bookmarks":[]}"#,
        )
        .unwrap();
        let mut suggestion = Suggestion {
            site: data.history[0].clone(),
            bookmarked: false,
        };
        assert_eq!(suggestion.site.visited_at, None);
        assert_eq!(suggestion.description(100), "Example");
        suggestion.site.visited_at = Some(100);
        for (elapsed, expected) in [
            (0, "just now"),
            (59, "just now"),
            (60, "1m ago"),
            (3599, "59m ago"),
            (3600, "1h ago"),
            (86400, "1 day ago"),
            (259200, "3 days ago"),
        ] {
            assert!(suggestion.description(100 + elapsed).ends_with(expected));
        }
        assert!(suggestion.description(0).ends_with("just now"));
        suggestion.bookmarked = true;
        assert_eq!(suggestion.description(1000), "Example");
        let roundtrip: Site =
            serde_json::from_str(&serde_json::to_string(&suggestion.site).unwrap()).unwrap();
        assert_eq!(roundtrip.visited_at, Some(100));
    }
    #[test]
    fn reload_tracks_external_changes_and_deletion_without_losing_good_data() {
        let dir = std::env::temp_dir().join(format!("browser-library-reload-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut reader = Library::load(&dir).unwrap();
        let mut writer = Library::load(&dir).unwrap();
        writer.visit("https://example.com", "External visit").unwrap();
        reader.reload().unwrap();
        assert_eq!(reader.suggestions("").len(), 1);
        let pointer = reader.data.history.as_ptr();
        reader.reload().unwrap();
        assert_eq!(reader.data.history.as_ptr(), pointer);
        fs::write(dir.join("library.json"), "invalid").unwrap();
        assert!(reader.reload().is_err());
        assert_eq!(reader.suggestions("").len(), 1);
        fs::remove_file(dir.join("library.json")).unwrap();
        reader.reload().unwrap();
        assert!(reader.suggestions("").is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn persist_merge_deduplicate_and_bound() {
        let dir = std::env::temp_dir().join(format!("browser-library-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut a = Library::load(&dir).unwrap();
        let mut b = Library::load(&dir).unwrap();
        a.visit("https://github.com", "GitHub").unwrap();
        assert_eq!(
            b.add_bookmark("https://github.com", "GitHub").unwrap(),
            Some(true)
        );
        a.visit("https://rust-lang.org", "Rust").unwrap();
        let mut a = Library::load(&dir).unwrap();
        let hits = a.suggestions("");
        assert_eq!(hits.len(), 2);
        assert!(hits[0].bookmarked);
        assert_eq!(a.suggestions("ghb").len(), 1);
        assert_eq!(
            a.add_bookmark("https://github.com", "GitHub").unwrap(),
            Some(false)
        );
        let reloaded = Library::load(&dir).unwrap();
        assert_eq!(reloaded.data.bookmarks.len(), 1);
        assert!(reloaded.data.history.iter().all(|s| s.visited_at.is_some()));
        // One shared fuzzy search returns both sources, without duplicating GitHub.
        let mixed = reloaded.suggestions("https");
        assert_eq!(mixed.len(), 2);
        assert!(mixed.iter().any(|s| s.bookmarked));
        assert!(mixed.iter().any(|s| !s.bookmarked));
        a.visit("about:blank", "Home").unwrap();
        assert_eq!(a.suggestions("").len(), 2);
        assert_eq!(
            site("https://user:secret@example.com/a", "Test")
                .unwrap()
                .url,
            "https://example.com/a"
        );
        a.update(|data| {
            data.history = (0..600)
                .map(|n| Site {
                    url: format!("https://example.com/{n}"),
                    title: n.to_string(),
                    visited_at: None,
                })
                .collect();
        })
        .unwrap();
        a.visit("https://example.com/new", "New").unwrap();
        assert_eq!(a.data.history.len(), HISTORY_LIMIT);
        assert_eq!(a.suggestions("").len(), 8);
        fs::write(dir.join("library.json"), "broken").unwrap();
        assert!(
            a.visit("https://example.com", "Don't overwrite corrupt data")
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(dir.join("library.json")).unwrap(),
            "broken"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
