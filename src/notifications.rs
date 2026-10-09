//! Stack of passive toasts: newest first, each with its own expiry.
use std::time::{Duration, Instant};

pub const MAX: usize = 3;
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u64,
    pub title: String,
    pub body: String,
    expires: Instant,
}
#[derive(Default)]
pub struct Stack {
    items: Vec<Notification>,
    next: u64,
}
/// Toasts are single-line rows.
fn line(s: &str) -> String {
    s.split(char::is_control)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
impl Stack {
    pub fn items(&self) -> &[Notification] {
        &self.items
    }
    pub fn push(&mut self, title: &str, body: &str, now: Instant, timeout: Duration) {
        self.next += 1;
        self.items.insert(
            0,
            Notification {
                id: self.next,
                title: line(title),
                body: line(body),
                expires: now + timeout,
            },
        );
        self.items.truncate(MAX);
    }
    /// Whether the stack changed.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|n| n.id != id);
        self.items.len() != before
    }
    /// Whether the stack changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|n| n.expires > now);
        self.items.len() != before
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newest_first_capped_and_expiring() {
        let now = Instant::now();
        let mut s = Stack::default();
        for i in 0..4 {
            s.push(
                &format!("t{i}"),
                "",
                now + Duration::from_secs(i),
                Duration::from_secs(5),
            );
        }
        let titles: Vec<_> = s.items().iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["t3", "t2", "t1"]);
        assert!(!s.expire(now + Duration::from_secs(5)));
        assert!(s.expire(now + Duration::from_secs(7)));
        assert_eq!(s.items().len(), 1);
        assert!(s.expire(now + Duration::from_secs(8)));
        assert!(s.items().is_empty());
    }
    #[test]
    fn dismiss_by_id() {
        let now = Instant::now();
        let mut s = Stack::default();
        s.push("a", "", now, Duration::from_secs(5));
        s.push("b", "", now, Duration::from_secs(5));
        let a = s.items()[1].id;
        assert!(s.dismiss(a));
        assert!(!s.dismiss(a));
        assert_eq!(s.items()[0].title, "b");
    }
    #[test]
    fn control_characters_become_spaces() {
        let mut s = Stack::default();
        s.push("a\nb", "c\r\n\td", Instant::now(), Duration::from_secs(1));
        assert_eq!(s.items()[0].title, "a b");
        assert_eq!(s.items()[0].body, "c d");
    }
}
