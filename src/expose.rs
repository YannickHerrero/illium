//! Exposé model: the cards shown for the managed windows of every workspace,
//! their layout on the surface, filtering and keyboard navigation. Pure state:
//! capturing, focusing and closing windows live in the platform layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: isize,
    pub title: String,
    /// Executable name without extension.
    pub app: String,
    pub workspace: u8,
    /// Width over height of the window's visible frame; the card keeps it.
    pub aspect: f32,
    pub minimized: bool,
    pub focused: bool,
}
/// Thumbnail rectangle in logical pixels of the surface; the label strip of
/// `LABEL` pixels sits right below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
/// Room kept for the filter box above the cards and the key hints below.
pub const TOP: f32 = 120.0;
pub const BOTTOM: f32 = 72.0;
pub const SIDE: f32 = 80.0;
/// Between cards of a row, and between rows (label strips included).
pub const GAP: f32 = 48.0;
pub const ROW_GAP: f32 = 40.0;
pub const LABEL: f32 = 64.0;
/// Aspect ratios outside this range would starve the other cards of a row.
const ASPECT_RANGE: (f32, f32) = (0.5, 3.0);
/// A lone card never fills the surface: the layout stays a gallery.
const MAX_HEIGHT_SHARE: f32 = 0.55;

pub fn clamp_aspect(aspect: f32) -> f32 {
    if aspect.is_finite() && aspect > 0.0 {
        aspect.clamp(ASPECT_RANGE.0, ASPECT_RANGE.1)
    } else {
        1.6
    }
}
/// Justified rows: every card of a row shares one height and keeps its own
/// aspect ratio. The row count giving the tallest cards wins, rows are filled
/// evenly (the first ones take the remainder) and centered, as is the block.
pub fn layout(aspects: &[f32], width: f32, height: f32) -> Vec<Slot> {
    let n = aspects.len();
    if n == 0 {
        return vec![];
    }
    let avail_w = (width - 2.0 * SIDE).max(1.0);
    let avail_h = (height - TOP - BOTTOM).max(1.0);
    let mut best: Option<(f32, Vec<usize>)> = None;
    for rows in 1..=n {
        let (base, extra) = (n / rows, n % rows);
        let sizes: Vec<usize> = (0..rows).map(|r| base + usize::from(r < extra)).collect();
        let mut h = ((avail_h - (rows as f32 - 1.0) * ROW_GAP) / rows as f32 - LABEL)
            .min(avail_h * MAX_HEIGHT_SHARE);
        let mut start = 0;
        for size in &sizes {
            let sum: f32 = aspects[start..start + size].iter().sum();
            h = h.min((avail_w - (*size as f32 - 1.0) * GAP) / sum);
            start += size;
        }
        if best.as_ref().is_none_or(|(tallest, _)| h > *tallest) {
            best = Some((h, sizes));
        }
    }
    let (h, sizes) = best.unwrap_or((1.0, vec![n]));
    let h = h.max(1.0).floor();
    let rows = sizes.len() as f32;
    let block_h = rows * (h + LABEL) + (rows - 1.0) * ROW_GAP;
    let y0 = TOP + ((avail_h - block_h) / 2.0).floor();
    let mut slots = Vec::with_capacity(n);
    let mut start = 0;
    for (row, size) in sizes.iter().enumerate() {
        let widths: Vec<f32> = aspects[start..start + size]
            .iter()
            .map(|a| (a * h).floor().max(1.0))
            .collect();
        let row_w = widths.iter().sum::<f32>() + (*size as f32 - 1.0) * GAP;
        let mut x = SIDE + ((avail_w - row_w) / 2.0).floor();
        let y = y0 + row as f32 * (h + LABEL + ROW_GAP);
        for w in widths {
            slots.push(Slot { x, y, w, h });
            x += w + GAP;
        }
        start += size;
    }
    slots
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
    /// One slot per shown card, from the last `arrange`.
    pub slots: Vec<Slot>,
}
impl Model {
    /// Opens on the focused window when it is listed.
    pub fn new(entries: Vec<Entry>) -> Self {
        let selected = entries.iter().position(|e| e.focused).unwrap_or(0);
        Self {
            shown: (0..entries.len()).collect(),
            entries,
            query: String::new(),
            selected,
            slots: vec![],
        }
    }
    /// Lays the shown cards out on a `width` by `height` surface.
    pub fn arrange(&mut self, width: f32, height: f32) {
        let aspects: Vec<f32> = self
            .shown
            .iter()
            .map(|i| clamp_aspect(self.entries[*i].aspect))
            .collect();
        self.slots = layout(&aspects, width, height);
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
    /// The card of the nearest row in that direction whose center is closest
    /// horizontally; nothing happens on the first or last row.
    fn vertical(&mut self, down: bool) {
        let Some(current) = self.slots.get(self.selected).copied() else {
            return;
        };
        let center = current.x + current.w / 2.0;
        let target = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if down {
                    s.y > current.y
                } else {
                    s.y < current.y
                }
            })
            .min_by(|(_, a), (_, b)| {
                let key = |s: &Slot| ((s.y - current.y).abs(), (s.x + s.w / 2.0 - center).abs());
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index);
        if let Some(index) = target {
            self.selected = index;
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
            aspect: 1.6,
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
    fn overlapping(a: &Slot, b: &Slot) -> bool {
        let (ah, bh) = (a.h + LABEL, b.h + LABEL);
        !(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + ah <= b.y || b.y + bh <= a.y)
    }
    #[test]
    fn layout_fits_keeps_aspects_and_never_overlaps() {
        for count in 1..=30 {
            let aspects: Vec<f32> = (0..count)
                .map(|i| clamp_aspect([1.6, 0.8, 2.4, 1.0][i % 4]))
                .collect();
            for (w, h) in [(1280.0, 720.0), (2560.0, 1440.0), (1024.0, 1366.0)] {
                let slots = layout(&aspects, w, h);
                assert_eq!(slots.len(), count);
                for (i, a) in slots.iter().enumerate() {
                    assert!(
                        a.x >= SIDE - 1.0 && a.x + a.w <= w - SIDE + 1.0,
                        "{a:?} in {w}x{h}"
                    );
                    assert!(
                        a.y >= TOP && a.y + a.h + LABEL <= h - BOTTOM + 1.0,
                        "{a:?} in {w}x{h}"
                    );
                    assert!(a.w >= 1.0 && a.h >= 1.0);
                    assert!(((a.w / a.h) - aspects[i]).abs() < 0.1 || a.h < 20.0);
                    for b in &slots[i + 1..] {
                        assert!(!overlapping(a, b), "{a:?} overlaps {b:?}");
                    }
                }
            }
        }
        assert!(layout(&[], 800.0, 600.0).is_empty());
        assert_eq!(clamp_aspect(f32::NAN), 1.6);
        assert_eq!(clamp_aspect(10.0), 3.0);
    }
    #[test]
    fn rows_share_a_height_and_are_centered() {
        let slots = layout(&[1.6; 5], 1600.0, 900.0);
        let first_row: Vec<&Slot> = slots.iter().filter(|s| s.y == slots[0].y).collect();
        assert_eq!(first_row.len(), 3, "the first row takes the remainder");
        assert!(slots.iter().all(|s| s.h == slots[0].h));
        let left = first_row[0].x;
        let right = first_row[2].x + first_row[2].w;
        assert_eq!(left, 1600.0 - right);
        let second_row: Vec<&Slot> = slots.iter().filter(|s| s.y > slots[0].y).collect();
        assert_eq!(second_row.len(), 2);
        assert!(second_row[0].x > left, "a partial row is centered too");
        let top = slots[0].y - TOP;
        let bottom = (900.0 - BOTTOM) - (second_row[0].y + second_row[0].h + LABEL);
        assert!((top - bottom).abs() <= 1.0);
    }
    #[test]
    fn opens_on_the_focused_window_and_navigates_with_wrapping() {
        let mut m = Model::new(entries());
        m.arrange(1600.0, 900.0);
        assert_eq!(m.selected_id(), Some(2));
        m.action(Action::Previous);
        m.action(Action::Previous);
        assert_eq!(m.selected_id(), Some(5));
        m.action(Action::Next);
        assert_eq!(m.selected_id(), Some(1));
        m.action(Action::Down);
        assert_eq!(m.selected_id(), Some(4), "nearest card of the row below");
        m.action(Action::Down);
        assert_eq!(m.selected_id(), Some(4), "no row below");
        m.action(Action::Up);
        assert!(m.selected < 3, "back to the first row");
        m.action(Action::Up);
        assert!(m.selected < 3, "no row above");
        m.select(2);
        m.action(Action::Down);
        assert_eq!(m.selected_id(), Some(5));
        assert_eq!(m.action(Action::Confirm), Outcome::Activate(5));
    }
    #[test]
    fn filter_matches_title_or_app_and_keeps_the_selection_when_possible() {
        let mut m = Model::new(entries());
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
        let mut m = Model::new(entries());
        m.action(Action::Text("zzz".into()));
        assert!(m.shown.is_empty());
        m.arrange(1600.0, 900.0);
        assert!(m.slots.is_empty());
        m.action(Action::Down);
        assert_eq!(m.action(Action::Confirm), Outcome::Close);
        assert_eq!(m.action(Action::Escape), Outcome::None);
        assert_eq!(m.shown.len(), 5);
        assert_eq!(m.action(Action::Escape), Outcome::Close);
    }
    #[test]
    fn closing_a_window_removes_its_card() {
        let mut m = Model::new(entries());
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
