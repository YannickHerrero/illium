pub mod demo;
pub mod leader;
pub mod launch;
pub mod library;
pub mod tabs;

use adblock::{Engine, request::Request};
use std::{collections::BTreeSet, path::Path};
use url::Url;

/// Only web URLs are accepted. Never dispatch arbitrary OS protocols from input.
pub fn address(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() || input == "about:blank" {
        return "about:blank".into();
    }
    if let Ok(url) = Url::parse(input)
        && matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
    {
        return url.into();
    }
    if !input.chars().any(char::is_whitespace)
        && !input.contains("://")
        && let Ok(url) = Url::parse(&format!("https://{input}"))
        && url
            .host_str()
            .is_some_and(|h| h.contains('.') || h == "localhost" || h.contains(':'))
        && url.username().is_empty()
    {
        return url.into();
    }
    let mut url = Url::parse("https://duckduckgo.com/").unwrap();
    url.query_pairs_mut().append_pair("q", input);
    url.into()
}

pub fn host(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_owned)
}

pub struct Blocker {
    engine: Engine,
    exceptions: BTreeSet<String>,
    pub blocked: u64,
}
impl Blocker {
    pub fn from_rules(rules: String) -> Self {
        Self {
            engine: Engine::new_with_list_text(rules),
            exceptions: BTreeSet::new(),
            blocked: 0,
        }
    }
    /// No network access at startup. Compile lists only when their content changes.
    pub fn load(dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        use std::hash::{Hash, Hasher};
        let mut rules = String::new();
        for name in ["easylist.txt", "easyprivacy.txt", "custom.txt"] {
            match std::fs::read_to_string(dir.join(name)) {
                Ok(text) => {
                    rules.push_str(&text);
                    rules.push('\n');
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        if rules.trim().is_empty() {
            return Err("No filter lists: run scripts/update-browser-filters.ps1 first".into());
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        rules.hash(&mut hash);
        // Bump the cache version when changing the engine dependency or parsing options.
        let cache = dir.join(format!("filters-adblock-0.13.3-{:x}.bin", hash.finish()));
        let mut blocker = Self::from_rules(String::new());
        let restored = std::fs::read(&cache)
            .ok()
            .is_some_and(|data| blocker.engine.deserialize(&data).is_ok());
        if !restored {
            blocker.engine = Engine::new_with_list_text(rules);
            // Cache is optional; inability to write must not disable filtering.
            let _ = std::fs::write(&cache, blocker.engine.serialize());
        }
        match std::fs::read(dir.join("exceptions.json")) {
            Ok(bytes) => blocker.exceptions = serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(blocker)
    }
    pub fn enabled(&self, page: &str) -> bool {
        self.exceptions.is_empty() || !host(page).is_some_and(|h| self.exceptions.contains(&h))
    }
    pub fn toggle(&mut self, page: &str, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let Some(host) = host(page) else {
            return Ok(());
        };
        let mut next = self.exceptions.clone();
        if !next.remove(&host) {
            next.insert(host);
        }
        let temp = dir.join("exceptions.json.tmp");
        std::fs::write(&temp, serde_json::to_vec(&next)?)?;
        std::fs::rename(temp, dir.join("exceptions.json"))?;
        self.exceptions = next;
        Ok(())
    }
    pub fn check(&mut self, url: &str, source: &str, kind: &str, method: &str, page: &str) -> bool {
        if !self.enabled(page) {
            return false;
        }
        let blocked = Request::new(url, source, kind, method)
            .ok()
            .is_some_and(|r| self.engine.check_network_request(&r).should_block());
        if blocked {
            self.blocked += 1;
        }
        blocked
    }
    /// Initial prototype: site-specific CSS only; no scriptlets or DOM polling.
    pub fn cosmetic_script(&self, page: &str) -> Option<String> {
        if !self.enabled(page) {
            return None;
        }
        let resources = self.engine.url_cosmetic_resources(page);
        if resources.hide_selectors.is_empty() {
            return None;
        }
        let css = resources
            .hide_selectors
            .into_iter()
            .map(|s| format!("{s} {{ display: none !important; }}"))
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!(
            "(()=>{{const s=document.createElement('style');s.textContent={};(document.head||document.documentElement).append(s)}})()",
            serde_json::to_string(&css).unwrap()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation() {
        assert_eq!(address("example.com/a"), "https://example.com/a");
        assert_eq!(address("localhost:8000"), "https://localhost:8000/");
        assert_eq!(address(""), "about:blank");
        assert_eq!(address("http://localhost:8000"), "http://localhost:8000/");
        for s in [
            "hello world",
            "javascript:alert(1)",
            "file:///secret",
            "a&b",
        ] {
            let u = Url::parse(&address(s)).unwrap();
            assert_eq!(u.host_str(), Some("duckduckgo.com"));
            assert_eq!(u.query_pairs().next().unwrap().1, s);
        }
    }
    #[test]
    fn filtering() {
        let mut b = Blocker::from_rules(
            "||ads.example^$third-party\n@@||ads.example/allowed.js\nnews.example##.advert".into(),
        );
        assert!(b.check(
            "https://ads.example/a.js",
            "https://news.example",
            "script",
            "GET",
            "https://news.example"
        ));
        assert!(!b.check(
            "https://ads.example/allowed.js",
            "https://news.example",
            "script",
            "GET",
            "https://news.example"
        ));
        assert!(!b.check(
            "https://ads.example/a.js",
            "https://ads.example",
            "script",
            "GET",
            "https://ads.example"
        ));
        assert!(
            b.cosmetic_script("https://news.example")
                .unwrap()
                .contains(".advert")
        );
        b.exceptions.insert("news.example".into());
        assert!(!b.check(
            "https://ads.example/a.js",
            "https://news.example",
            "script",
            "GET",
            "https://news.example"
        ));
        assert!(b.cosmetic_script("https://news.example").is_none());
        assert_eq!(b.blocked, 1);
    }
    #[test]
    fn local_integration_rules() {
        let mut b = Blocker::from_rules(include_str!("../../../tests/browser/custom.txt").into());
        let page = "http://127.0.0.1:8765/index.html";
        assert!(b.check("http://127.0.0.1:8765/ads.js", page, "script", "GET", page));
        assert!(!b.check(
            "http://127.0.0.1:8765/ads.js?allowed=1",
            page,
            "script",
            "GET",
            page
        ));
        assert!(b.cosmetic_script(page).unwrap().contains(".advert"));
    }
    #[test]
    fn cache_and_exceptions_survive_restart() {
        let dir =
            std::env::temp_dir().join(format!("winarchy-browser-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("custom.txt"), "||ads.example^").unwrap();
        let mut b = Blocker::load(&dir).unwrap();
        b.toggle("https://news.example", &dir).unwrap();
        let b = Blocker::load(&dir).unwrap();
        assert!(!b.enabled("https://news.example/article"));
        assert!(b.enabled("https://other.example"));
        // Changing list contents must invalidate the compiled cache.
        std::fs::write(dir.join("custom.txt"), "||different.example^").unwrap();
        let mut b = Blocker::load(&dir).unwrap();
        assert!(b.check(
            "https://different.example/ad",
            "https://other.example",
            "image",
            "GET",
            "https://other.example"
        ));
        assert!(!b.check(
            "https://ads.example/ad",
            "https://other.example",
            "image",
            "GET",
            "https://other.example"
        ));
        std::fs::remove_file(dir.join("custom.txt")).unwrap();
        assert!(Blocker::load(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
