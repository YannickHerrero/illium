//! `synthgrid`: create a grid which fills with characters dissolving into the final text.
use crate::engine::*;
use std::collections::HashMap;

const GRID_GRADIENT_STOPS: [&str; 2] = ["CC00CC", "ffffff"];
const GRID_GRADIENT_STEPS: usize = 12;
const GRID_GRADIENT_DIRECTION: Direction = Direction::Diagonal;
const TEXT_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const TEXT_GRADIENT_STEPS: usize = 12;
const TEXT_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const GRID_ROW_SYMBOL: char = '─';
const GRID_COLUMN_SYMBOL: char = '│';
const TEXT_GENERATION_SYMBOLS: [char; 3] = ['░', '▒', '▓'];
const MAX_ACTIVE_BLOCKS: f64 = 0.1;

const UPDATE_GROUP_TRACKER: u32 = 0;

#[derive(PartialEq)]
enum LineDirection {
    Horizontal,
    Vertical,
}

struct GridLine {
    direction: LineDirection,
    collapsed: Vec<CharId>,
    extended: Vec<CharId>,
}

impl GridLine {
    fn new(
        t: &mut Terminal,
        origin: Coord,
        direction: LineDirection,
        mapping: &HashMap<Coord, Color>,
    ) -> Self {
        let (symbol, coords): (char, Vec<Coord>) = match direction {
            LineDirection::Horizontal => (
                GRID_ROW_SYMBOL,
                (t.canvas.left..=t.canvas.right)
                    .map(|column| Coord::new(column, origin.row))
                    .collect(),
            ),
            LineDirection::Vertical => (
                GRID_COLUMN_SYMBOL,
                (t.canvas.bottom..t.canvas.top)
                    .map(|row| Coord::new(origin.column, row))
                    .collect(),
            ),
        };
        let mut characters = vec![];
        for coord in coords {
            let id = t.add_character(symbol, Coord::new(0, 0));
            let ch = &mut t.chars[id];
            let scene = ch.animation.scene();
            ch.animation
                .get(scene)
                .add_frame(symbol, 1, ColorPair::fg(mapping[&coord]));
            t.activate_scene(id, scene);
            let ch = &mut t.chars[id];
            ch.layer = 2;
            ch.motion.set_coordinate(coord);
            characters.push(id);
        }
        Self {
            direction,
            collapsed: characters,
            extended: vec![],
        }
    }
    fn count(&self) -> usize {
        if self.direction == LineDirection::Horizontal {
            3
        } else {
            1
        }
    }
    fn extend(&mut self, t: &mut Terminal) {
        for _ in 0..self.count() {
            if !self.collapsed.is_empty() {
                let id = self.collapsed.remove(0);
                t.set_visible(id, true);
                self.extended.push(id);
            }
        }
    }
    fn collapse(&mut self, t: &mut Terminal) {
        if self.collapsed.is_empty() {
            self.extended.reverse();
        }
        for _ in 0..self.count() {
            if !self.extended.is_empty() {
                let id = self.extended.remove(0);
                t.set_visible(id, false);
                self.collapsed.push(id);
            }
        }
    }
    fn is_extended(&self) -> bool {
        self.collapsed.is_empty()
    }
    fn is_collapsed(&self) -> bool {
        self.extended.is_empty()
    }
}

#[derive(PartialEq)]
enum Phase {
    GridExpand,
    AddChars,
    Collapse,
    Complete,
}

pub struct SynthGrid {
    pending_groups: Vec<(usize, Vec<CharId>)>,
    grid_lines: Vec<GridLine>,
    group_tracker: Vec<i64>,
    phase: Phase,
    total_group_count: usize,
    active_groups: usize,
    active: Active,
}

fn find_even_gap(dimension: i32) -> i32 {
    let dimension = dimension - 2;
    if dimension <= 0 {
        return 0;
    }
    let potential: Vec<i32> = (5..=dimension)
        .rev()
        .filter(|i| dimension % i <= 1)
        .collect();
    if potential.is_empty() {
        return 4;
    }
    let mut best = potential[0];
    for &gap in &potential[1..] {
        if (gap - dimension / 5).abs() < (best - dimension / 5).abs() {
            best = gap;
        }
    }
    best
}

impl SynthGrid {
    pub fn new(t: &mut Terminal) -> Self {
        let grid_gradient = Gradient::new(&colors(&GRID_GRADIENT_STOPS), &[GRID_GRADIENT_STEPS]);
        let c = t.canvas.clone();
        let grid_mapping = grid_gradient.build_coordinate_color_mapping(
            1,
            c.top,
            1,
            c.right,
            GRID_GRADIENT_DIRECTION,
        );
        let text_gradient = Gradient::new(&colors(&TEXT_GRADIENT_STOPS), &[TEXT_GRADIENT_STEPS]);
        let text_mapping = text_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            TEXT_GRADIENT_DIRECTION,
        );
        let mut final_colors: HashMap<CharId, ColorPair> = HashMap::new();
        for id in t.input_characters() {
            let ch = &t.chars[id];
            let colors = if ch.input_symbol != ' ' {
                ColorPair::fg(text_mapping[&ch.input_coord])
            } else {
                ColorPair::default()
            };
            final_colors.insert(id, colors);
        }
        let mut grid_lines = vec![
            GridLine::new(
                t,
                Coord::new(c.left, c.bottom),
                LineDirection::Horizontal,
                &grid_mapping,
            ),
            GridLine::new(
                t,
                Coord::new(c.left, c.top),
                LineDirection::Horizontal,
                &grid_mapping,
            ),
            GridLine::new(
                t,
                Coord::new(c.left, c.bottom),
                LineDirection::Vertical,
                &grid_mapping,
            ),
            GridLine::new(
                t,
                Coord::new(c.right, c.bottom),
                LineDirection::Vertical,
                &grid_mapping,
            ),
        ];
        let (row_gap, column_gap) = if c.top > 2 * c.right {
            let row_gap = find_even_gap(c.top) + 1;
            (row_gap, row_gap * 2)
        } else {
            let column_gap = find_even_gap(c.right) + 1;
            (column_gap.div_euclid(2), column_gap)
        };
        let mut row_indexes = vec![];
        let mut column_indexes = vec![];
        for row_index in (c.bottom + row_gap..c.top).step_by(row_gap.max(1) as usize) {
            if c.top - row_index < 2 {
                continue;
            }
            row_indexes.push(row_index);
            grid_lines.push(GridLine::new(
                t,
                Coord::new(c.left, row_index),
                LineDirection::Horizontal,
                &grid_mapping,
            ));
        }
        for column_index in (c.left + column_gap..c.right).step_by(column_gap.max(1) as usize) {
            if c.right - column_index < 2 {
                continue;
            }
            column_indexes.push(column_index);
            grid_lines.push(GridLine::new(
                t,
                Coord::new(column_index, c.bottom),
                LineDirection::Vertical,
                &grid_mapping,
            ));
        }
        row_indexes.push(c.top + 1);
        column_indexes.push(c.right + 1);
        let mut pending_groups: Vec<(usize, Vec<CharId>)> = vec![];
        let mut prev_row_index = 1;
        for &row_index in &row_indexes {
            let mut row_index = row_index;
            let mut prev_column_index = 1;
            for &column_index in &column_indexes {
                if row_index == c.top {
                    row_index += 1;
                }
                let mut block = vec![];
                for row in prev_row_index..row_index {
                    for column in prev_column_index..column_index {
                        if let Some(id) = t.get_character_by_input_coord(Coord::new(column, row)) {
                            block.push(id);
                        }
                    }
                }
                if !block.is_empty() {
                    pending_groups.push((pending_groups.len(), block));
                }
                prev_column_index = column_index;
            }
            prev_row_index = row_index;
        }
        let mut group_tracker = vec![0; pending_groups.len()];
        for (group_number, group) in &pending_groups {
            group_tracker[*group_number] = 0;
            for &id in group {
                let rng = &mut t.rng;
                let ch = &mut t.chars[id];
                let dissolve = ch.animation.scene();
                for _ in 0..rng.randint(15, 30) {
                    let symbol = *rng.choice(&TEXT_GENERATION_SYMBOLS);
                    let color = *rng.choice(&text_gradient.spectrum);
                    ch.animation
                        .get(dissolve)
                        .add_frame(symbol, 2, ColorPair::fg(color));
                }
                let symbol = ch.input_symbol;
                ch.animation.get(dissolve).add_frame(
                    symbol,
                    1,
                    final_colors.get(&id).copied().unwrap_or_default(),
                );
                ch.register(
                    Event::SceneComplete,
                    Caller::Scene(dissolve),
                    Action::Callback(UPDATE_GROUP_TRACKER, *group_number as i64),
                );
                t.activate_scene(id, dissolve);
            }
        }
        t.rng.shuffle(&mut pending_groups);
        let total_group_count = pending_groups.len();
        let mut active = Active::default();
        if total_group_count == 0 {
            for id in t.input_characters() {
                t.set_visible(id, true);
                active.add(id);
            }
        }
        Self {
            pending_groups,
            grid_lines,
            group_tracker,
            phase: Phase::GridExpand,
            total_group_count,
            active_groups: 0,
            active,
        }
    }
}

impl Effect for SynthGrid {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_groups.is_empty() && self.active.is_empty() && self.phase == Phase::Complete
        {
            return false;
        }
        match self.phase {
            Phase::GridExpand => {
                if !self.grid_lines.iter().all(GridLine::is_extended) {
                    for line in &mut self.grid_lines {
                        if !line.is_extended() {
                            line.extend(t);
                        }
                    }
                } else {
                    self.phase = Phase::AddChars;
                }
            }
            Phase::AddChars => {
                if !self.pending_groups.is_empty()
                    && (self.active_groups as f64)
                        < self.total_group_count as f64 * MAX_ACTIVE_BLOCKS
                {
                    let (group_number, group) = self.pending_groups.remove(0);
                    for id in group {
                        t.set_visible(id, true);
                        self.active.add(id);
                        self.group_tracker[group_number] += 1;
                    }
                }
                if self.pending_groups.is_empty()
                    && self.active.is_empty()
                    && self.active_groups == 0
                {
                    self.phase = Phase::Collapse;
                }
            }
            Phase::Collapse => {
                if !self.grid_lines.iter().all(GridLine::is_collapsed) {
                    for line in &mut self.grid_lines {
                        if !line.is_collapsed() {
                            line.collapse(t);
                        }
                    }
                } else {
                    self.phase = Phase::Complete;
                }
            }
            Phase::Complete => {}
        }
        self.active.update(t);
        for (_, tag, group_number) in t.take_callbacks() {
            if tag == UPDATE_GROUP_TRACKER {
                self.group_tracker[group_number as usize] -= 1;
            }
        }
        self.active_groups = self.group_tracker.iter().filter(|&&n| n != 0).count();
        true
    }
}
