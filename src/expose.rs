//! Exposé model: the cards shown for the managed windows of every workspace,
//! their grid on the surface, filtering and keyboard navigation. Pure state:
//! capturing, focusing and closing windows live in the platform layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: isize,
    pub title: String,
    /// Executable name without extension.
    pub app: String,
    pub workspace: u8,
    pub minimized: bool,
    pub focused: bool,
}
/// Card rectangle in logical pixels of the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
/// Thumbnail area aspect ratio; the label strip sits below it.
pub const ASPECT: f32 = 16.0 / 10.0;
pub const LABEL: f32 = 40.0;
pub const GAP: f32 = 24.0;
pub const MARGIN: f32 = 64.0;
const MAX_WIDTH: f32 = 640.0;

/// Grid for `count` cards on a `width` by `height` surface: the column count
/// giving the widest cards wins; the grid and a partial last row are centered.
pub fn grid(count: usize, width: f32, height: f32) -> (usize, Vec<Slot>) {
    if count == 0 {
        return (1, vec![]);
    }
    let (mut cols, mut card_w) = (1, 0.0f32);
    for candidate in 1..=count {
        let rows = count.div_ceil(candidate);
        let avail_w = width - 2.0 * MARGIN - (candidate as f32 - 1.0) * GAP;
        let avail_h = height - 2.0 * MARGIN - (rows as f32 - 1.0) * GAP;
        let w = (avail_w / candidate as f32)
            .min((avail_h / rows as f32 - LABEL) * ASPECT)
            .min(MAX_WIDTH);
        if w > card_w {
            (cols, card_w) = (candidate, w);
        }
    }
    let card_w = card_w.max(1.0).floor();
    let card_h = (card_w / ASPECT + LABEL).floor();
    let rows = count.div_ceil(cols);
    let total_h = rows as f32 * card_h + (rows as f32 - 1.0) * GAP;
    let y0 = ((height - total_h) / 2.0).floor();
    let mut slots = Vec::with_capacity(count);
    for row in 0..rows {
        let in_row = (count - row * cols).min(cols);
        let row_w = in_row as f32 * card_w + (in_row as f32 - 1.0) * GAP;
        let x0 = ((width - row_w) / 2.0).floor();
        for column in 0..in_row {
            slots.push(Slot {
                x: x0 + column as f32 * (card_w + GAP),
                y: y0 + row as f32 * (card_h + GAP),
                w: card_w,
                h: card_h,
            });
        }
    }
    (cols, slots)
}
/// Case-insensitive subsequence match, like the launcher's fuzzy search.
pub fn matches(query: &str, text: &str) -> bool {
    let mut chars = text.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .all(|q| chars.any(|c| c == q))
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Escape,
    Confirm,
    Backspace,
    Clear,
    Next,
    Previous,
    Up,
    Down,
    Text(String),
    /// The configured `window close` chord: close the selected window.
    CloseWindow,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    None,
    Close,
    Activate(isize),
    CloseWindow(isize),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub entries: Vec<Entry>,
    pub query: String,
    /// Indices into `entries` that match the query, in display order.
    pub shown: Vec<usize>,
    /// Index into `shown`.
    pub selected: usize,
    pub cols: usize,
}
impl Model {
    /// Opens on the focused window when it is listed.
    pub fn new(entries: Vec<Entry>, cols: usize) -> Self {
        let selected = entries.iter().position(|e| e.focused).unwrap_or(0);
        Self {
            shown: (0..entries.len()).collect(),
            entries,
            query: String::new(),
            selected,
            cols: cols.max(1),
        }
    }
    pub fn selected_id(&self) -> Option<isize> {
        self.shown
            .get(self.selected)
            .map(|index| self.entries[*index].id)
    }
    pub fn select(&mut self, shown: usize) {
        if shown < self.shown.len() {
            self.selected = shown;
        }
    }
    /// Recomputes the visible cards; the selection follows its window when
    /// that window still matches, otherwise it returns to the first card.
    fn refilter(&mut self) {
        let keep = self.selected_id();
        self.refilter_keeping(keep);
    }
    fn refilter_keeping(&mut self, keep: Option<isize>) {
        self.shown = (0..self.entries.len())
            .filter(|index| {
                let e = &self.entries[*index];
                self.query.is_empty()
                    || matches(&self.query, &e.title)
                    || matches(&self.query, &e.app)
            })
            .collect();
        self.selected = keep
            .and_then(|id| self.shown.iter().position(|i| self.entries[*i].id == id))
            .unwrap_or(0);
    }
    /// Drops a window that was closed from the exposé.
    pub fn remove(&mut self, id: isize) {
        let keep = self.selected_id().filter(|selected| *selected != id);
        let position = self.selected;
        self.entries.retain(|e| e.id != id);
        self.refilter_keeping(keep);
        if keep.is_none() {
            self.selected = position.min(self.shown.len().saturating_sub(1));
        }
    }
    fn step(&mut self, delta: isize) {
        let n = self.shown.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(n) as usize;
        }
    }
    fn vertical(&mut self, down: bool) {
        let n = self.shown.len();
        if n == 0 {
            return;
        }
        let target = if down {
            self.selected + self.cols
        } else {
            self.selected.wrapping_sub(self.cols)
        };
        if target < n {
            self.selected = target;
        }
    }
    pub fn action(&mut self, action: Action) -> Outcome {
        match action {
            Action::Escape if !self.query.is_empty() => {
                self.query.clear();
                self.refilter();
            }
            Action::Escape => return Outcome::Close,
            Action::Confirm => {
                return match self.selected_id() {
                    Some(id) => Outcome::Activate(id),
                    None => Outcome::Close,
                };
            }
            Action::CloseWindow => {
                if let Some(id) = self.selected_id() {
                    return Outcome::CloseWindow(id);
                }
            }
            Action::Backspace => {
                self.query.pop();
                self.refilter();
            }
            Action::Clear => {
                self.query.clear();
                self.refilter();
            }
            Action::Text(text) => {
                self.query.push_str(&text);
                self.refilter();
            }
            Action::Next => self.step(1),
            Action::Previous => self.step(-1),
            Action::Down => self.vertical(true),
            Action::Up => self.vertical(false),
        }
        Outcome::None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: isize, title: &str, app: &str, workspace: u8) -> Entry {
        Entry {
            id,
            title: title.into(),
            app: app.into(),
            workspace,
            minimized: false,
            focused: id == 2,
        }
    }
    fn entries() -> Vec<Entry> {
        vec![
            entry(1, "docs - Firefox", "firefox", 1),
            entry(2, "main.rs - Code", "code", 1),
            entry(3, "WezTerm", "wezterm", 2),
            entry(4, "Teams", "ms-teams", 3),
            entry(5, "Inbox", "olk", 3),
        ]
    }
    #[test]
    fn grid_fits_and_never_overlaps() {
        for count in 1..=40 {
            for (w, h) in [(1280.0, 720.0), (2560.0, 1440.0), (1024.0, 1366.0)] {
                let (cols, slots) = grid(count, w, h);
                assert_eq!(slots.len(), count);
                assert!(cols >= 1 && cols <= count);
                for (i, a) in slots.iter().enumerate() {
                    assert!(a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= w && a.y + a.h <= h);
                    assert!(a.w >= 1.0 && a.h > LABEL);
                    for b in &slots[i + 1..] {
                        assert!(
                            a.x + a.w <= b.x
                                || b.x + b.w <= a.x
                                || a.y + a.h <= b.y
                                || b.y + b.h <= a.y
                        );
                    }
                }
            }
        }
        assert!(grid(0, 800.0, 600.0).1.is_empty());
    }
    #[test]
    fn grid_is_centered_with_a_centered_last_row() {
        let (cols, slots) = grid(5, 1600.0, 900.0);
        assert_eq!(cols, 3);
        let first_row_left = slots[0].x;
        let first_row_right = slots[2].x + slots[2].w;
        assert_eq!(first_row_left, 1600.0 - first_row_right);
        let last_row_left = slots[3].x;
        let last_row_right = slots[4].x + slots[4].w;
        assert_eq!(last_row_left, 1600.0 - last_row_right);
        assert!(last_row_left > first_row_left);
        let top = slots[0].y;
        let bottom = slots[4].y + slots[4].h;
        assert_eq!(top, 900.0 - bottom);
    }
    #[test]
    fn opens_on_the_focused_window_and_navigates_with_wrapping() {
        let mut m = Model::new(entries(), 3);
        assert_eq!(m.selected_id(), Some(2));
        m.action(Action::Previous);
        m.action(Action::Previous);
        assert_eq!(m.selected_id(), Some(5));
        m.action(Action::Next);
        assert_eq!(m.selected_id(), Some(1));
        m.action(Action::Down);
        assert_eq!(m.selected_id(), Some(4));
        m.action(Action::Down);
        assert_eq!(m.selected_id(), Some(4), "no row below");
        m.action(Action::Up);
        assert_eq!(m.selected_id(), Some(1));
        m.action(Action::Up);
        assert_eq!(m.selected_id(), Some(1), "no row above");
        assert_eq!(m.action(Action::Confirm), Outcome::Activate(1));
    }
    #[test]
    fn filter_matches_title_or_app_and_keeps_the_selection_when_possible() {
        let mut m = Model::new(entries(), 3);
        m.action(Action::Text("te".into()));
        assert_eq!(
            m.shown.iter().map(|i| m.entries[*i].id).collect::<Vec<_>>(),
            vec![3, 4]
        );
        assert_eq!(m.selected_id(), Some(3), "focused window filtered out");
        m.action(Action::Next);
        assert_eq!(m.selected_id(), Some(4));
        m.action(Action::Text("a".into()));
        assert_eq!(m.selected_id(), Some(4), "selection follows its window");
        m.action(Action::Backspace);
        assert_eq!(m.selected_id(), Some(4));
        assert!(matches("OLK", "olk"));
        assert!(!matches("xyz", "Inbox"));
    }
    #[test]
    fn escape_clears_the_filter_before_closing() {
        let mut m = Model::new(entries(), 3);
        m.action(Action::Text("zzz".into()));
        assert!(m.shown.is_empty());
        assert_eq!(m.action(Action::Confirm), Outcome::Close);
        assert_eq!(m.action(Action::Escape), Outcome::None);
        assert_eq!(m.shown.len(), 5);
        assert_eq!(m.action(Action::Escape), Outcome::Close);
    }
    #[test]
    fn closing_a_window_removes_its_card() {
        let mut m = Model::new(entries(), 3);
        assert_eq!(m.action(Action::CloseWindow), Outcome::CloseWindow(2));
        m.remove(2);
        assert_eq!(m.entries.len(), 4);
        assert_eq!(m.selected_id(), Some(3));
        for id in [1, 3, 4, 5] {
            m.remove(id);
        }
        assert_eq!(m.action(Action::CloseWindow), Outcome::None);
        assert_eq!(m.action(Action::Confirm), Outcome::Close);
    }
}
