//! Port of `terminaltexteffects.engine.terminal` (release 0.15.0), as run by
//! Omarchy: `--canvas-width 0 --canvas-height 0 --anchor-canvas c --anchor-text c`,
//! so the canvas is the whole terminal and the text is centered in it.
use super::animation::SceneId;
use super::character::{CharId, Character};
use super::geometry::Coord;
use super::graphics::{Color, ColorPair};
use super::motion::PathId;
use super::rng::Rng;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug)]
pub struct Canvas {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
    pub center_row: i32,
    pub center_column: i32,
    pub center: Coord,
    pub width: i32,
    pub height: i32,
    pub text_left: i32,
    pub text_right: i32,
    pub text_top: i32,
    pub text_bottom: i32,
    pub text_width: i32,
    pub text_height: i32,
    pub text_center_row: i32,
    pub text_center_column: i32,
    pub text_center: Coord,
}

impl Canvas {
    fn new(top: i32, right: i32) -> Self {
        let (bottom, left) = (1, 1);
        let mut center_row = (top / 2).max(bottom);
        if top % 2 == 1 && top > 1 {
            center_row += 1;
        }
        let mut center_column = (right / 2).max(left);
        if right % 2 == 1 && right > 1 {
            center_column += 1;
        }
        Self {
            top,
            right,
            bottom,
            left,
            center_row,
            center_column,
            center: Coord::new(center_column, center_row),
            width: right,
            height: top,
            text_left: 0,
            text_right: 0,
            text_top: 0,
            text_bottom: 0,
            text_width: 0,
            text_height: 0,
            text_center_row: 0,
            text_center_column: 0,
            text_center: Coord::new(0, 0),
        }
    }
    pub fn coord_is_in_canvas(&self, c: Coord) -> bool {
        self.left <= c.column && c.column <= self.right && self.bottom <= c.row && c.row <= self.top
    }
    pub fn coord_is_in_text(&self, c: Coord) -> bool {
        self.text_left <= c.column
            && c.column <= self.text_right
            && self.text_bottom <= c.row
            && c.row <= self.text_top
    }
    pub fn random_column(&self, rng: &mut Rng, within_text: bool) -> i32 {
        if within_text {
            rng.randint(self.text_left as i64, self.text_right as i64) as i32
        } else {
            rng.randint(self.left as i64, self.right as i64) as i32
        }
    }
    pub fn random_row(&self, rng: &mut Rng, within_text: bool) -> i32 {
        if within_text {
            rng.randint(self.text_bottom as i64, self.text_top as i64) as i32
        } else {
            rng.randint(self.bottom as i64, self.top as i64) as i32
        }
    }
    pub fn random_coord(&self, rng: &mut Rng, outside_scope: bool, within_text: bool) -> Coord {
        if outside_scope {
            let options = [
                Coord::new(self.random_column(rng, false), self.top + 1),
                Coord::new(self.random_column(rng, false), self.bottom - 1),
                Coord::new(self.left - 1, self.random_row(rng, false)),
                Coord::new(self.right + 1, self.random_row(rng, false)),
            ];
            return *rng.choice(&options);
        }
        Coord::new(
            self.random_column(rng, within_text),
            self.random_row(rng, within_text),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterSort {
    Random,
    TopToBottomLeftToRight,
    TopToBottomRightToLeft,
    BottomToTopLeftToRight,
    BottomToTopRightToLeft,
    OutsideRowToMiddle,
    MiddleRowToOutside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterGroup {
    ColumnLeftToRight,
    ColumnRightToLeft,
    RowTopToBottom,
    RowBottomToTop,
    DiagonalTopLeftToBottomRight,
    DiagonalBottomLeftToTopRight,
    DiagonalTopRightToBottomLeft,
    DiagonalBottomRightToTopLeft,
    CenterToOutside,
    OutsideToCenter,
}

/// Which character lists `get_characters` draws from.
#[derive(Clone, Copy, Debug)]
pub struct Select {
    pub input: bool,
    pub inner_fill: bool,
    pub outer_fill: bool,
    pub added: bool,
}

impl Select {
    pub const INPUT: Self = Self {
        input: true,
        inner_fill: false,
        outer_fill: false,
        added: false,
    };
    pub const FILL: Self = Self {
        input: false,
        inner_fill: true,
        outer_fill: true,
        added: false,
    };
    pub const ALL_CELLS: Self = Self {
        input: true,
        inner_fill: true,
        outer_fill: true,
        added: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Cell {
    pub symbol: char,
    pub colors: ColorPair,
}

pub struct Terminal {
    pub canvas: Canvas,
    pub chars: Vec<Character>,
    input: Vec<CharId>,
    inner_fill: Vec<CharId>,
    outer_fill: Vec<CharId>,
    added: Vec<CharId>,
    by_input_coord: HashMap<Coord, CharId>,
    pub rng: Rng,
    /// `terminal_background_color`, used by effects to fade to the background.
    pub background: Color,
    callbacks: Vec<(CharId, u32, i64)>,
}

impl Terminal {
    pub fn new(text: &str, columns: i32, rows: i32, seed: u64) -> Self {
        let lines: Vec<Vec<char>> = {
            let mut lines: Vec<Vec<char>> = text
                .split('\n')
                .map(|l| {
                    let mut l: Vec<char> = l.trim_end_matches('\r').chars().collect();
                    while l.last() == Some(&' ') {
                        l.pop();
                    }
                    l
                })
                .collect();
            while lines.last().is_some_and(|l| l.is_empty()) {
                lines.pop();
            }
            lines
        };
        let mut canvas = Canvas::new(rows.max(1), columns.max(1));
        let mut chars = vec![];
        let height = lines.len() as i32;
        for (row, line) in lines.iter().enumerate() {
            for (column, &symbol) in line.iter().enumerate() {
                if symbol != ' ' {
                    let id = chars.len();
                    chars.push(Character::new(
                        id,
                        symbol,
                        Coord::new(column as i32 + 1, height - row as i32),
                    ));
                }
            }
        }
        // Canvas._anchor_text with anchor "c".
        let input_width = chars
            .iter()
            .map(|c| c.input_coord.column)
            .max()
            .unwrap_or(1);
        let input_height = chars.iter().map(|c| c.input_coord.row).max().unwrap_or(1);
        let column_delta = if input_width != canvas.width {
            canvas.center_column - input_width / 2
        } else {
            0
        };
        let row_delta = if input_height != canvas.height {
            canvas.center_row - input_height / 2
        } else {
            0
        };
        for c in &mut chars {
            let anchored = Coord::new(
                c.input_coord.column + column_delta,
                c.input_coord.row + row_delta,
            );
            c.set_input_coord(anchored);
        }
        chars.retain(|c| canvas.coord_is_in_canvas(c.input_coord));
        for (i, c) in chars.iter_mut().enumerate() {
            c.id = i;
        }
        if !chars.is_empty() {
            canvas.text_left = chars.iter().map(|c| c.input_coord.column).min().unwrap();
            canvas.text_right = chars.iter().map(|c| c.input_coord.column).max().unwrap();
            canvas.text_top = chars.iter().map(|c| c.input_coord.row).max().unwrap();
            canvas.text_bottom = chars.iter().map(|c| c.input_coord.row).min().unwrap();
            canvas.text_width = (canvas.text_right - canvas.text_left + 1).max(1);
            canvas.text_height = (canvas.text_top - canvas.text_bottom + 1).max(1);
            canvas.text_center_row =
                canvas.text_bottom + (canvas.text_top - canvas.text_bottom) / 2;
            canvas.text_center_column =
                canvas.text_left + (canvas.text_right - canvas.text_left) / 2;
            canvas.text_center = Coord::new(canvas.text_center_column, canvas.text_center_row);
        }
        let input: Vec<CharId> = (0..chars.len()).collect();
        let mut by_input_coord: HashMap<Coord, CharId> =
            chars.iter().map(|c| (c.input_coord, c.id)).collect();
        let (mut inner_fill, mut outer_fill) = (vec![], vec![]);
        for row in 1..=canvas.top {
            for column in 1..=canvas.right {
                let coord = Coord::new(column, row);
                if by_input_coord.contains_key(&coord) {
                    continue;
                }
                let id = chars.len();
                let mut fill = Character::new(id, ' ', coord);
                fill.is_fill = true;
                chars.push(fill);
                by_input_coord.insert(coord, id);
                if canvas.coord_is_in_text(coord) {
                    inner_fill.push(id);
                } else {
                    outer_fill.push(id);
                }
            }
        }
        Self {
            canvas,
            chars,
            input,
            inner_fill,
            outer_fill,
            added: vec![],
            by_input_coord,
            rng: Rng::new(seed),
            background: Color::hex("000000"),
            callbacks: vec![],
        }
    }

    pub fn add_character(&mut self, symbol: char, coord: Coord) -> CharId {
        let id = self.chars.len();
        self.chars.push(Character::new(id, symbol, coord));
        self.added.push(id);
        id
    }

    fn selected(&self, sel: Select) -> Vec<CharId> {
        let mut all = vec![];
        if sel.input {
            all.extend_from_slice(&self.input);
        }
        if sel.inner_fill {
            all.extend_from_slice(&self.inner_fill);
        }
        if sel.outer_fill {
            all.extend_from_slice(&self.outer_fill);
        }
        if sel.added {
            all.extend_from_slice(&self.added);
        }
        all
    }

    pub fn get_characters(&mut self, sel: Select, sort: CharacterSort) -> Vec<CharId> {
        let mut all = self.selected(sel);
        let key = |t: &Self, id: &CharId| t.chars[*id].input_coord;
        all.sort_by_key(|id| {
            let c = key(self, id);
            (-c.row, c.column)
        });
        match sort {
            CharacterSort::Random => self.rng.shuffle(&mut all),
            CharacterSort::TopToBottomLeftToRight => {}
            CharacterSort::BottomToTopRightToLeft => all.reverse(),
            CharacterSort::BottomToTopLeftToRight | CharacterSort::TopToBottomRightToLeft => {
                all.sort_by_key(|id| {
                    let c = key(self, id);
                    (c.row, c.column)
                });
                if sort == CharacterSort::TopToBottomRightToLeft {
                    all.reverse();
                }
            }
            CharacterSort::OutsideRowToMiddle | CharacterSort::MiddleRowToOutside => {
                let mut deque: std::collections::VecDeque<CharId> = all.into();
                all = (0..deque.len())
                    .map(|i| {
                        if i % 2 == 0 {
                            deque.pop_front().unwrap()
                        } else {
                            deque.pop_back().unwrap()
                        }
                    })
                    .collect();
                if sort == CharacterSort::MiddleRowToOutside {
                    all.reverse();
                }
            }
        }
        all
    }

    pub fn input_characters(&mut self) -> Vec<CharId> {
        self.get_characters(Select::INPUT, CharacterSort::TopToBottomLeftToRight)
    }

    pub fn get_characters_grouped(
        &self,
        grouping: CharacterGroup,
        sel: Select,
    ) -> Vec<Vec<CharId>> {
        use CharacterGroup::*;
        let mut all = self.selected(sel);
        all.sort_by_key(|id| {
            let c = self.chars[*id].input_coord;
            (c.row, c.column)
        });
        let coord = |id: &CharId| self.chars[*id].input_coord;
        let collect = |range: std::ops::RangeInclusive<i32>,
                       key: &dyn Fn(Coord) -> i32|
         -> Vec<Vec<CharId>> {
            range
                .map(|k| {
                    all.iter()
                        .copied()
                        .filter(|id| key(coord(id)) == k)
                        .collect::<Vec<_>>()
                })
                .filter(|g| !g.is_empty())
                .collect()
        };
        let canvas = &self.canvas;
        let mut groups = match grouping {
            ColumnLeftToRight | ColumnRightToLeft => collect(0..=canvas.right, &|c| c.column),
            RowTopToBottom | RowBottomToTop => collect(0..=canvas.top, &|c| c.row),
            DiagonalBottomLeftToTopRight | DiagonalTopRightToBottomLeft => {
                collect(0..=canvas.top + canvas.right, &|c| c.row + c.column)
            }
            DiagonalTopLeftToBottomRight | DiagonalBottomRightToTopLeft => collect(
                canvas.left - canvas.top..=canvas.right - canvas.bottom,
                &|c| c.column - c.row,
            ),
            CenterToOutside | OutsideToCenter => {
                let center = canvas.text_center;
                let mut map: std::collections::BTreeMap<i32, Vec<CharId>> = Default::default();
                for id in &all {
                    let c = coord(id);
                    let d = (c.column - center.column).abs() + (c.row - center.row).abs();
                    map.entry(d).or_default().push(*id);
                }
                map.into_values().collect()
            }
        };
        if matches!(
            grouping,
            ColumnRightToLeft
                | RowTopToBottom
                | DiagonalTopRightToBottomLeft
                | DiagonalBottomRightToTopLeft
                | OutsideToCenter
        ) {
            groups.reverse();
        }
        groups
    }

    pub fn get_character_by_input_coord(&self, coord: Coord) -> Option<CharId> {
        self.by_input_coord.get(&coord).copied()
    }

    pub fn set_visible(&mut self, id: CharId, visible: bool) {
        self.chars[id].visible = visible;
    }

    fn collect(&mut self, id: CharId) {
        let fired = std::mem::take(&mut self.chars[id].fired);
        self.callbacks
            .extend(fired.into_iter().map(|(tag, arg)| (id, tag, arg)));
    }
    pub fn tick(&mut self, id: CharId) {
        self.chars[id].tick();
        self.collect(id);
    }
    pub fn activate_scene(&mut self, id: CharId, scene: SceneId) {
        self.chars[id].activate_scene(scene);
        self.collect(id);
    }
    pub fn activate_path(&mut self, id: CharId, path: PathId) {
        self.chars[id].activate_path(path);
        self.collect(id);
    }
    /// Callbacks fired since the last call, oldest first.
    pub fn take_callbacks(&mut self) -> Vec<(CharId, u32, i64)> {
        std::mem::take(&mut self.callbacks)
    }

    /// `_update_terminal_state`: visible characters painted by layer.
    pub fn render(&self) -> Vec<Cell> {
        let (w, h) = (self.canvas.right, self.canvas.top);
        let mut cells = vec![
            Cell {
                symbol: ' ',
                colors: ColorPair::default()
            };
            (w * h) as usize
        ];
        let mut visible: Vec<&Character> = self.chars.iter().filter(|c| c.visible).collect();
        visible.sort_by_key(|c| c.layer);
        for c in visible {
            let Coord { column, row } = c.motion.current_coord;
            if (1..=h).contains(&row) && (1..=w).contains(&column) {
                let visual = c.animation.current;
                cells[((h - row) * w + column - 1) as usize] = Cell {
                    symbol: visual.symbol,
                    colors: visual.colors,
                };
            }
        }
        cells
    }
}

/// `set` of active characters, iterated in ascending id order.
#[derive(Default, Clone, Debug)]
pub struct Active(pub BTreeSet<CharId>);

impl Active {
    pub fn add(&mut self, id: CharId) {
        self.0.insert(id);
    }
    pub fn remove(&mut self, id: CharId) {
        self.0.remove(&id);
    }
    pub fn contains(&self, id: CharId) -> bool {
        self.0.contains(&id)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// `BaseEffectIterator.update`: tick every active character, then drop
    /// the ones that finished.
    pub fn update(&mut self, t: &mut Terminal) {
        for &id in &self.0 {
            t.tick(id);
        }
        self.0.retain(|&id| t.chars[id].is_active());
    }
}

/// One TTE effect iterator. `next` advances one frame and returns `false`
/// when the iterator would raise `StopIteration`.
pub trait Effect {
    fn next(&mut self, t: &mut Terminal) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centers_text_like_tte() {
        let t = Terminal::new("ab\n c", 10, 5, 1);
        assert_eq!(t.canvas.center, Coord::new(5, 3));
        let coords: Vec<_> = t
            .input
            .iter()
            .map(|&i| (t.chars[i].input_symbol, t.chars[i].input_coord))
            .collect();
        // input width 2, height 2: deltas 5 - 1 and 3 - 1.
        assert_eq!(
            coords,
            [
                ('a', Coord::new(5, 4)),
                ('b', Coord::new(6, 4)),
                ('c', Coord::new(6, 3))
            ]
        );
        assert_eq!(t.chars.len(), 50);
        assert_eq!(t.canvas.text_center, Coord::new(5, 3));
    }
    #[test]
    fn sorts_and_groups() {
        let mut t = Terminal::new("ab\ncd", 2, 2, 1);
        let order: Vec<char> = t
            .get_characters(Select::INPUT, CharacterSort::BottomToTopLeftToRight)
            .into_iter()
            .map(|i| t.chars[i].input_symbol)
            .collect();
        assert_eq!(order, ['c', 'd', 'a', 'b']);
        let rows = t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::INPUT);
        assert_eq!(rows.len(), 2);
        assert_eq!(t.chars[rows[0][0]].input_symbol, 'a');
    }
}
