//! `slice`: slices the input in half and slides it into place from opposite
//! directions (the default vertical slice).
use crate::engine::*;

const MOVEMENT_SPEED: f64 = 0.25;
const MOVEMENT_EASING: Ease = Ease::InOutExpo;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

pub struct Slice {
    active: Active,
}

impl Slice {
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
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            let color = mapping[&ch.input_coord];
            let symbol = ch.input_symbol;
            ch.animation
                .set_appearance(Some(symbol), ColorPair::fg(color));
        }
        let rows = t.get_characters_grouped(CharacterGroup::RowBottomToTop, Select::INPUT);
        let center_column = t.canvas.text_center_column;
        let (top, bottom) = (t.canvas.top, t.canvas.bottom);
        let mut active = Active::default();
        let mut slide_in = |t: &mut Terminal, id: CharId, start_row: i32| {
            let ch = &mut t.chars[id];
            let input_coord = ch.input_coord;
            ch.motion
                .set_coordinate(Coord::new(input_coord.column, start_row));
            let path = ch.motion.path(MOVEMENT_SPEED, Some(MOVEMENT_EASING));
            ch.motion.get(path).waypoint(input_coord);
            t.activate_path(id, path);
            active.add(id);
        };
        for (row_index, row) in rows.iter().enumerate() {
            for &id in row {
                if t.chars[id].input_coord.column <= center_column {
                    slide_in(t, id, top + 1);
                }
            }
            for &id in &rows[rows.len() - 1 - row_index] {
                if t.chars[id].input_coord.column > center_column {
                    slide_in(t, id, bottom - 1);
                }
            }
        }
        for &id in &active.0 {
            t.set_visible(id, true);
        }
        Self { active }
    }
}

impl Effect for Slice {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() {
            return false;
        }
        self.active.update(t);
        true
    }
}
