//! Omarchy image picker interaction/geometry port (see docs/licenses/omarchy.md).
//! Pure state: browsing never writes preferences or applies a theme.
mod disk_cache;
pub mod loader;
pub mod render;
pub const EXPANDED_WIDTH: f32 = 768.0;
pub const EXPANDED_HEIGHT: f32 = 475.0;
pub const SLICE_WIDTH: f32 = 108.0;
pub const SLICE_HEIGHT: f32 = 432.0;
pub const STEP: f32 = 78.0;
pub const SKEW: f32 = 28.0;
pub const CARD_HEIGHT: f32 = 609.0;
pub const CAROUSEL_WIDTH: f32 = 1782.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub index: usize,
    pub selected: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub z: i32,
}
impl Card {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Qt's labelForPath, for identifiers (not the theme.toml display name).
pub fn label(id: &str) -> String {
    let mut result = String::new();
    let mut boundary = true;
    let mut separator = false;
    for ch in id.chars() {
        if ch == '-' || ch == '_' {
            if !separator {
                result.push(' ');
            }
            separator = true;
            boundary = true;
        } else {
            separator = false;
            if boundary && ch.is_ascii_alphanumeric() {
                result.push(ch.to_ascii_uppercase());
            } else {
                result.push(ch);
            }
            // JS /\b\w/ uses ASCII word boundaries, not Unicode title casing.
            boundary = !ch.is_ascii_alphanumeric();
        }
    }
    result
}

#[derive(Clone, Default, Debug)]
pub struct Model {
    pub ids: Vec<String>,
    pub filter: String,
    selected: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Previous,
    Next,
    Text(String),
    Backspace,
    DeleteWord,
    Clear,
    Escape,
    Confirm,
    Click(usize),
}
#[derive(Debug, PartialEq)]
pub enum Outcome {
    None,
    Cancel,
    Apply(String),
}
impl Model {
    pub fn new(ids: Vec<String>, active: &str) -> Self {
        let selected = ids.iter().position(|id| id == active).unwrap_or(0);
        Self {
            ids,
            selected,
            filter: String::new(),
        }
    }
    pub fn matches(&self, index: usize) -> bool {
        self.ids.get(index).is_some_and(|id| {
            let needle = self.filter.to_lowercase();
            id.to_lowercase().contains(&needle) || label(id).to_lowercase().contains(&needle)
        })
    }
    pub fn matches_indices(&self) -> Vec<usize> {
        (0..self.ids.len()).filter(|i| self.matches(*i)).collect()
    }
    pub fn selected(&self) -> Option<usize> {
        self.matches(self.selected).then_some(self.selected)
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.selected().map(|i| self.ids[i].as_str())
    }
    pub fn current_label(&self) -> String {
        self.selected_id().map(label).unwrap_or_else(|| {
            if self.filter.is_empty() {
                String::new()
            } else {
                "No matches".into()
            }
        })
    }
    fn refilter(&mut self) {
        if !self.matches(self.selected)
            && let Some(i) = self.matches_indices().first()
        {
            self.selected = *i;
        }
    }
    pub fn replace(&mut self, ids: Vec<String>) {
        let selected = self.ids.get(self.selected).cloned().unwrap_or_default();
        self.ids = ids;
        self.selected = self.ids.iter().position(|id| *id == selected).unwrap_or(0);
        self.refilter();
    }
    pub fn action(&mut self, action: Action) -> Outcome {
        match action {
            Action::Previous | Action::Next => {
                let matches = self.matches_indices();
                if !matches.is_empty() {
                    let p = matches
                        .iter()
                        .position(|i| *i == self.selected)
                        .unwrap_or(0);
                    let p = if action == Action::Previous {
                        (p + matches.len() - 1) % matches.len()
                    } else {
                        (p + 1) % matches.len()
                    };
                    self.selected = matches[p];
                }
            }
            Action::Text(text) => {
                self.filter.extend(text.chars().filter(|c| !c.is_control()));
                self.refilter();
            }
            Action::Backspace => {
                self.filter.pop();
                self.refilter();
            }
            Action::DeleteWord => {
                self.filter = self.filter.trim_end().to_owned();
                while self
                    .filter
                    .chars()
                    .last()
                    .is_some_and(|c| !c.is_whitespace())
                {
                    self.filter.pop();
                }
                self.refilter();
            }
            Action::Clear => {
                self.filter.clear();
                self.refilter();
            }
            Action::Escape => {
                if self.filter.is_empty() {
                    return Outcome::Cancel;
                }
                self.filter.clear();
                self.refilter();
            }
            Action::Confirm => {
                return self
                    .selected_id()
                    .map(|id| Outcome::Apply(id.to_owned()))
                    .unwrap_or(Outcome::Cancel);
            }
            Action::Click(i) => {
                if self.matches(i) {
                    if self.selected == i {
                        return Outcome::Apply(self.ids[i].clone());
                    }
                    self.selected = i;
                }
            }
        }
        Outcome::None
    }
    /// Back-to-front drawing order; selected is always last. Hit tests reverse it.
    pub fn cards(&self, screen_width: f32, screen_height: f32) -> Vec<Card> {
        let matches = self.matches_indices();
        let position = matches
            .iter()
            .position(|i| *i == self.selected)
            .unwrap_or(0) as i32;
        let center_x = (screen_width - EXPANDED_WIDTH) / 2.0;
        let top = (screen_height - CARD_HEIGHT) / 2.0 + 30.0;
        let mut cards: Vec<_> = matches
            .into_iter()
            .enumerate()
            .filter_map(|(p, index)| {
                let relative = p as i32 - position;
                if relative.abs() > 16 {
                    return None;
                }
                let selected = index == self.selected;
                let (width, height) = if selected {
                    (EXPANDED_WIDTH, EXPANDED_HEIGHT)
                } else {
                    (SLICE_WIDTH, SLICE_HEIGHT)
                };
                let x = if selected {
                    center_x
                } else if relative < 0 {
                    center_x + relative as f32 * STEP
                } else {
                    center_x + EXPANDED_WIDTH - 30.0 + (relative - 1) as f32 * STEP
                };
                // Include the raster border, but do not decode cards clipped
                // entirely by the monitor. Keep the original layout/z-order.
                if x + width + render::PAD <= 0.0 || x - render::PAD >= screen_width {
                    return None;
                }
                Some(Card {
                    index,
                    selected,
                    x,
                    y: top + (EXPANDED_HEIGHT - height) / 2.0,
                    width,
                    height,
                    z: if selected {
                        100
                    } else {
                        50 - relative.abs().min(40)
                    },
                })
            })
            .collect();
        cards.sort_by_key(|c| c.z);
        cards
    }
    pub fn click(&mut self, x: f32, y: f32, width: f32, height: f32) -> Outcome {
        if let Some(card) = self
            .cards(width, height)
            .iter()
            .rev()
            .find(|c| c.contains(x, y))
        {
            return self.action(Action::Click(card.index));
        }
        let card_width = (width - 80.0).min(CAROUSEL_WIDTH + 40.0);
        let left = (width - card_width) / 2.0;
        let top = (height - CARD_HEIGHT) / 2.0;
        if !self.ids.is_empty()
            && x >= left
            && x < left + card_width
            && y >= top
            && y < top + CARD_HEIGHT
        {
            Outcome::None
        } else {
            Outcome::Cancel
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Model {
        Model::new(
            ["catppuccin-latte", "nord", "tokyo-night"]
                .map(String::from)
                .to_vec(),
            "nord",
        )
    }
    #[test]
    fn labels_and_substrings_not_fuzzy() {
        assert_eq!(label("tokyo--night_test"), "Tokyo Night Test");
        let mut m = model();
        m.action(Action::Text("TOKYO N".into()));
        assert_eq!(m.current_label(), "Tokyo Night");
        m.action(Action::Clear);
        m.action(Action::Text("tn".into()));
        assert_eq!(m.current_label(), "No matches");
        assert_eq!(m.action(Action::Confirm), Outcome::Cancel);
    }
    #[test]
    fn wraps_selection_and_separates_browse_from_apply() {
        let mut m = model();
        assert_eq!(m.action(Action::Next), Outcome::None);
        assert_eq!(m.selected(), Some(2));
        m.action(Action::Next);
        assert_eq!(m.selected(), Some(0));
        m.action(Action::Previous);
        assert_eq!(m.selected(), Some(2));
        assert_eq!(m.action(Action::Click(1)), Outcome::None);
        assert_eq!(m.action(Action::Click(1)), Outcome::Apply("nord".into()));
    }
    #[test]
    fn escape_and_editing() {
        let mut m = model();
        m.action(Action::Text("tokyo night  ".into()));
        m.action(Action::DeleteWord);
        assert_eq!(m.filter, "tokyo ");
        m.action(Action::Backspace);
        assert_eq!(m.filter, "tokyo");
        m.action(Action::Text("é".into()));
        m.action(Action::Backspace);
        assert_eq!(m.filter, "tokyo");
        assert_eq!(m.action(Action::Escape), Outcome::None);
        assert_eq!(m.action(Action::Escape), Outcome::Cancel);
        assert_eq!(m.selected_id(), Some("tokyo-night"));
    }
    #[test]
    fn keeps_matching_selection_and_survives_catalog_removal() {
        let mut m = model();
        m.action(Action::Text("o".into()));
        assert_eq!(m.selected_id(), Some("nord"));
        m.replace(vec!["tokyo-night".into()]);
        assert_eq!(m.selected_id(), Some("tokyo-night"));
        m.replace(vec![]);
        assert_eq!(m.selected(), None);
        assert_eq!(m.action(Action::Next), Outcome::None);
        assert_eq!(m.action(Action::Confirm), Outcome::Cancel);
    }
    #[test]
    fn geometry_and_rectangular_hit_testing_match_qml() {
        let mut m = model();
        let cards = m.cards(1920.0, 1080.0);
        let center = cards.last().unwrap();
        assert_eq!(
            (center.x, center.y, center.width, center.height),
            (576.0, 265.5, 768.0, 475.0)
        );
        assert_eq!(cards[0].x, 498.0);
        assert_eq!(cards[1].x, 1314.0);
        // Transparent top left corner still selects/applies (QML MouseArea).
        assert_eq!(
            m.click(577.0, 266.0, 1920.0, 1080.0),
            Outcome::Apply("nord".into())
        );
        assert_eq!(m.click(960.0, 820.0, 1920.0, 1080.0), Outcome::None);
        assert_eq!(m.click(0.0, 0.0, 1920.0, 1080.0), Outcome::Cancel);
    }
    #[test]
    fn only_nearby_cards_but_navigation_covers_entire_catalog() {
        let mut m = Model::new((0..100).map(|i| format!("theme-{i}")).collect(), "theme-50");
        let cards = m.cards(1920.0, 1080.0);
        assert_eq!(cards.len(), 17);
        assert!(cards.iter().all(|c| c.x + c.width + render::PAD > 0.0
            && c.x - render::PAD < 1920.0));
        assert_eq!(m.cards(5000.0, 1080.0).len(), 33);
        assert!(m.cards(800.0, 600.0).len() < cards.len());
        m.action(Action::Text("99".into()));
        assert_eq!(m.cards(800.0, 600.0).len(), 1);
        assert_eq!(m.cards(800.0, 600.0)[0].width, 768.0);
    }
}
