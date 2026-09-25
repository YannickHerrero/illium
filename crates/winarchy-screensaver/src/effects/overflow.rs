//! `overflow`: input text overflows and scrolls the terminal in a random order
//! until eventually appearing ordered.
use crate::engine::*;

const OVERFLOW_GRADIENT_STOPS: [&str; 3] = ["f2ebc0", "8dbfb3", "f2ebc0"];
const OVERFLOW_CYCLES_RANGE: (i64, i64) = (2, 4);
const OVERFLOW_SPEED: i64 = 3;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

struct Row {
    characters: Vec<CharId>,
    is_final: bool,
}

impl Row {
    fn move_up(&self, t: &mut Terminal) {
        for &id in &self.characters {
            let motion = &mut t.chars[id].motion;
            let current = motion.current_coord;
            motion.set_coordinate(Coord::new(current.column, current.row + 1));
        }
    }
    fn setup(&self, t: &mut Terminal) {
        for &id in &self.characters {
            let ch = &mut t.chars[id];
            let column = ch.input_coord.column;
            ch.motion.set_coordinate(Coord::new(column, 0));
        }
    }
    fn set_color(&self, t: &mut Terminal, fg: Color) {
        for &id in &self.characters {
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            ch.animation.set_appearance(Some(symbol), ColorPair::fg(fg));
        }
    }
    fn row(&self, t: &Terminal) -> i32 {
        t.chars[self.characters[0]].motion.current_coord.row
    }
}

pub struct Overflow {
    pending_rows: Vec<Row>,
    active_rows: Vec<Row>,
    delay: i64,
    overflow_gradient: Gradient,
    active: Active,
}

impl Overflow {
    pub fn new(t: &mut Terminal) -> Self {
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let (lower, upper) = OVERFLOW_CYCLES_RANGE;
        let mut rows = t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::INPUT);
        let mut pending_rows = vec![];
        if upper > 0 {
            for _ in 0..t.rng.randint(lower, upper) {
                t.rng.shuffle(&mut rows);
                for row in &rows {
                    let characters = row
                        .iter()
                        .map(|&id| {
                            let (symbol, coord) =
                                (t.chars[id].input_symbol, t.chars[id].input_coord);
                            t.add_character(symbol, coord)
                        })
                        .collect();
                    pending_rows.push(Row {
                        characters,
                        is_final: false,
                    });
                }
            }
        }
        for row in t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::ALL_CELLS) {
            for &id in &row {
                let ch = &mut t.chars[id];
                let color = mapping
                    .get(&ch.input_coord)
                    .copied()
                    .unwrap_or(Color::hex("000000"));
                let symbol = ch.animation.current.symbol;
                ch.animation
                    .set_appearance(Some(symbol), ColorPair::fg(color));
            }
            pending_rows.push(Row {
                characters: row,
                is_final: true,
            });
        }
        let steps = (t.canvas.top / (OVERFLOW_GRADIENT_STOPS.len() as i32 - 1).max(1)).max(1);
        let overflow_gradient = Gradient::new(&colors(&OVERFLOW_GRADIENT_STOPS), &[steps as usize]);
        Self {
            pending_rows,
            active_rows: vec![],
            delay: 0,
            overflow_gradient,
            active: Active::default(),
        }
    }
}

impl Effect for Overflow {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_rows.is_empty() {
            return false;
        }
        if self.delay == 0 {
            for _ in 0..t.rng.randint(1, OVERFLOW_SPEED) {
                if self.pending_rows.is_empty() {
                    continue;
                }
                let spectrum = &self.overflow_gradient.spectrum;
                for row in &self.active_rows {
                    row.move_up(t);
                    if !row.is_final {
                        let index = (row.row(t) as usize).min(spectrum.len() - 1);
                        row.set_color(t, spectrum[index]);
                    }
                }
                let next_row = self.pending_rows.remove(0);
                next_row.setup(t);
                next_row.move_up(t);
                if !next_row.is_final {
                    next_row.set_color(t, spectrum[0]);
                }
                for &id in &next_row.characters {
                    t.set_visible(id, true);
                }
                self.active_rows.push(next_row);
            }
            self.delay = t.rng.randint(0, 3);
        } else {
            self.delay -= 1;
        }
        let top = t.canvas.top;
        self.active_rows.retain(|row| row.row(t) <= top);
        self.active.update(t);
        true
    }
}
