//! Session-local tab identity, selection and fuzzy search (no WebView handles).
use crate::library::fuzzy;

pub type TabId = u64;
const CLOSED_LIMIT: usize = 20;
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub home: bool,
    pub pinned: bool,
    pub muted: bool,
    pub audible: bool,
    pub last_active: u64,
}
impl Tab {
    pub fn label(&self) -> &str {
        if self.home {
            "Nouvel onglet"
        } else if self.title.is_empty() {
            &self.url
        } else {
            &self.title
        }
    }
    pub fn age(&self, now: u64) -> String {
        match now.saturating_sub(self.last_active) {
            0..60 => "maintenant".into(),
            age @ 60..3600 => format!("{} min", age / 60),
            age @ 3600..86400 => format!("{} h", age / 3600),
            age => format!("{} j", age / 86400),
        }
    }
}
#[derive(Default)]
pub struct Tabs {
    entries: Vec<Tab>,
    active: Option<TabId>,
    next_id: TabId,
    closed: Vec<Tab>,
}
impl Tabs {
    pub fn entries(&self) -> &[Tab] {
        &self.entries
    }
    pub fn active(&self) -> Option<TabId> {
        self.active
    }
    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.entries.iter().find(|tab| tab.id == id)
    }
    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.entries.iter_mut().find(|tab| tab.id == id)
    }
    pub fn add(&mut self, url: &str, now: u64) -> TabId {
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(Tab {
            id,
            url: url.into(),
            title: String::new(),
            home: url == "about:blank",
            pinned: false,
            muted: false,
            audible: false,
            last_active: now,
        });
        id
    }
    pub fn activate(&mut self, id: TabId, now: u64) -> bool {
        let Some(tab) = self.get_mut(id) else {
            return false;
        };
        tab.last_active = now;
        self.active = Some(id);
        true
    }
    pub fn adjacent(&self, direction: i32) -> Option<TabId> {
        if self.entries.is_empty() {
            return None;
        }
        let current = self
            .entries
            .iter()
            .position(|tab| Some(tab.id) == self.active)
            .unwrap_or(0) as i32;
        Some(self.entries[(current + direction).rem_euclid(self.entries.len() as i32) as usize].id)
    }
    /// Pinned tabs are protected against accidental close. The active neighbor
    /// is chosen by stable list order, never by fuzzy-result position.
    pub fn close(&mut self, id: TabId) -> Option<Tab> {
        let index = self
            .entries
            .iter()
            .position(|tab| tab.id == id && !tab.pinned)?;
        let tab = self.entries.remove(index);
        if self.active == Some(id) {
            self.active = self
                .entries
                .get(index.min(self.entries.len().saturating_sub(1)))
                .map(|tab| tab.id);
        }
        self.closed.push(tab.clone());
        if self.closed.len() > CLOSED_LIMIT {
            self.closed.remove(0);
        }
        Some(tab)
    }
    /// Remove an initialization failure without adding it to recently closed.
    pub fn discard(&mut self, id: TabId) {
        self.entries.retain(|tab| tab.id != id);
        if self.active == Some(id) {
            self.active = self.entries.first().map(|tab| tab.id);
        }
    }
    pub fn recently_closed(&self) -> Option<&Tab> {
        self.closed.last()
    }
    pub fn finish_reopen(&mut self) {
        self.closed.pop();
    }
    pub fn toggle_pin(&mut self, id: TabId) {
        if let Some(tab) = self.get_mut(id) {
            tab.pinned = !tab.pinned;
        }
        self.entries.sort_by_key(|tab| !tab.pinned);
    }
    pub fn search(&self, query: &str) -> Vec<Tab> {
        let mut matches: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(position, tab)| {
                fuzzy(query, &format!("{} {}", tab.label(), tab.url))
                    .map(|score| (score, position, tab.clone()))
            })
            .collect();
        matches.sort_by_key(|(score, position, _)| (*score, *position));
        matches.into_iter().map(|(_, _, tab)| tab).collect()
    }
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_wrap_close_and_last_tab() {
        let mut tabs = Tabs::default();
        let a = tabs.add("https://a.test", 0);
        let b = tabs.add("https://b.test", 0);
        tabs.activate(a, 10);
        assert_eq!(tabs.adjacent(-1), Some(b));
        tabs.activate(b, 20);
        assert_eq!(tabs.adjacent(1), Some(a));
        tabs.close(a).unwrap();
        assert_eq!(tabs.active(), Some(b));
        tabs.close(b).unwrap();
        assert_eq!(tabs.active(), None);
        assert_eq!(tabs.adjacent(1), None);
        assert_eq!(tabs.recently_closed().unwrap().id, b);
        let replacement = tabs.add("about:blank", 30);
        assert_ne!(replacement, a);
        assert_ne!(replacement, b);
    }
    #[test]
    fn pin_search_and_stable_identity() {
        let mut tabs = Tabs::default();
        let a = tabs.add("https://example.test/first", 0);
        let b = tabs.add("https://example.test/second", 0);
        tabs.get_mut(b).unwrap().title = "Rust documentation".into();
        tabs.activate(a, 10);
        tabs.toggle_pin(b);
        assert_eq!(tabs.entries()[0].id, b);
        assert_eq!(tabs.active(), Some(a));
        assert!(tabs.close(b).is_none());
        assert_eq!(tabs.search("rstdoc")[0].id, b);
        assert_eq!(tabs.search("second")[0].id, b);
        assert!(tabs.search("zzzz").is_empty());
        tabs.toggle_pin(b);
        tabs.close(b).unwrap();
        assert!(tabs.get_mut(b).is_none()); // stale callbacks cannot mutate another tab
        assert_eq!(tabs.active(), Some(a));
    }
    #[test]
    fn closed_is_bounded_and_failures_are_not_reopened() {
        let mut tabs = Tabs::default();
        for _ in 0..30 {
            let id = tabs.add("about:blank", 0);
            tabs.close(id);
        }
        assert_eq!(tabs.closed.len(), CLOSED_LIMIT);
        let last = tabs.recently_closed().unwrap().id;
        let failed = tabs.add("https://failed.test", 0);
        tabs.discard(failed);
        assert_eq!(tabs.recently_closed().unwrap().id, last);
        tabs.finish_reopen();
        assert_ne!(tabs.recently_closed().unwrap().id, last);
    }
}
