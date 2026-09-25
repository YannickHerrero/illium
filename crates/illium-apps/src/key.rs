//! Keys as the applications see them, independent of the toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Tab,
}
impl Key {
    /// Slint reports special keys as private-use characters; text keys are
    /// their own character.
    #[cfg(windows)]
    pub fn from_slint(text: &str) -> Option<Self> {
        use slint::platform::Key as K;
        let c = text.chars().next()?;
        let special = |k: K| char::from(k) == c;
        Some(if special(K::Escape) {
            Self::Escape
        } else if special(K::Return) {
            Self::Enter
        } else if special(K::Backspace) {
            Self::Backspace
        } else if special(K::UpArrow) {
            Self::Up
        } else if special(K::DownArrow) {
            Self::Down
        } else if special(K::LeftArrow) {
            Self::Left
        } else if special(K::RightArrow) {
            Self::Right
        } else if special(K::PageUp) {
            Self::PageUp
        } else if special(K::PageDown) {
            Self::PageDown
        } else if special(K::Home) {
            Self::Home
        } else if special(K::End) {
            Self::End
        } else if special(K::Tab) {
            Self::Tab
        } else if c.is_control() {
            return None;
        } else {
            Self::Char(c)
        })
    }
}
/// Moves a cursor over `count` rows; `page` is the number of visible rows.
pub fn step(cursor: usize, count: usize, key: Key, page: usize) -> Option<usize> {
    if count == 0 {
        return Some(0);
    }
    let last = count - 1;
    Some(match key {
        Key::Down | Key::Char('j') => (cursor + 1).min(last),
        Key::Up | Key::Char('k') => cursor.saturating_sub(1),
        Key::PageDown => (cursor + page.max(1)).min(last),
        Key::PageUp => cursor.saturating_sub(page.max(1)),
        Key::Home => 0,
        Key::End | Key::Char('G') => last,
        _ => return None,
    })
}
/// Case-insensitive unless the pattern has an upper-case letter, as in yazi.
pub fn matches(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if pattern.chars().any(char::is_uppercase) {
        text.contains(pattern)
    } else {
        text.to_lowercase().contains(pattern)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_steps() {
        assert_eq!(step(0, 0, Key::Down, 10), Some(0));
        assert_eq!(step(0, 5, Key::Down, 10), Some(1));
        assert_eq!(step(4, 5, Key::Char('j'), 10), Some(4));
        assert_eq!(step(0, 5, Key::Up, 10), Some(0));
        assert_eq!(step(2, 50, Key::PageDown, 10), Some(12));
        assert_eq!(step(2, 50, Key::PageUp, 10), Some(0));
        assert_eq!(step(2, 50, Key::Char('G'), 10), Some(49));
        assert_eq!(step(30, 50, Key::Home, 10), Some(0));
        assert_eq!(step(0, 5, Key::Char('x'), 10), None);
    }
    #[test]
    fn smart_case() {
        assert!(matches("", "anything"));
        assert!(matches("code", "Code.exe"));
        assert!(!matches("Code", "code.exe"));
        assert!(matches("Code", "Code.exe"));
    }
}
