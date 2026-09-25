//! `rain`: rain characters from the top of the canvas.
use crate::engine::*;
use std::collections::BTreeMap;

const RAIN_COLORS: [&str; 8] = [
    "00315C", "004C8F", "0075DB", "3F91D9", "78B9F2", "9AC8F5", "B8D8F8", "E3EFFC",
];
const MOVEMENT_SPEED: (f64, f64) = (0.33, 0.57);
const RAIN_SYMBOLS: [char; 5] = ['o', '.', ',', '*', '|'];
const FINAL_GRADIENT_STOPS: [&str; 3] = ["488bff", "b2e7de", "57eaf7"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;
const MOVEMENT_EASING: Ease = Ease::InQuart;

pub struct Rain {
    pending: Vec<CharId>,
    group_by_row: BTreeMap<i32, Vec<CharId>>,
    active: Active,
}

impl Rain {
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
        let rain_colors = colors(&RAIN_COLORS);
        let top = t.canvas.top;
        let chars = t.input_characters();
        for &id in &chars {
            let rng = &mut t.rng;
            let raindrop_color = *rng.choice(&rain_colors);
            let symbol = *rng.choice(&RAIN_SYMBOLS);
            let speed = rng.uniform(MOVEMENT_SPEED.0, MOVEMENT_SPEED.1);
            let ch = &mut t.chars[id];
            let final_color = mapping[&ch.input_coord];
            let rain = ch.animation.scene();
            ch.animation
                .get(rain)
                .add_frame(symbol, 1, ColorPair::fg(raindrop_color));
            let fade = ch.animation.scene();
            let gradient = Gradient::new(&[raindrop_color, final_color], &[7]);
            let input_symbol = ch.input_symbol;
            ch.animation.get(fade).apply_gradient_to_symbols(
                &[input_symbol],
                3,
                Some(&gradient),
                None,
            );
            let input_coord = ch.input_coord;
            ch.motion
                .set_coordinate(Coord::new(input_coord.column, top));
            let path = ch.motion.path(speed, Some(MOVEMENT_EASING));
            ch.motion.get(path).waypoint(input_coord);
            ch.register(
                Event::PathComplete,
                Caller::Path(path),
                Action::ActivateScene(fade),
            );
            t.activate_scene(id, rain);
            t.activate_path(id, path);
        }
        let mut sorted = chars;
        sorted.sort_by_key(|&id| t.chars[id].input_coord.row);
        let mut group_by_row: BTreeMap<i32, Vec<CharId>> = BTreeMap::new();
        for id in sorted {
            group_by_row
                .entry(t.chars[id].input_coord.row)
                .or_default()
                .push(id);
        }
        Self {
            pending: vec![],
            group_by_row,
            active: Active::default(),
        }
    }
}

impl Effect for Rain {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.group_by_row.is_empty() && self.active.is_empty() && self.pending.is_empty() {
            return false;
        }
        if self.pending.is_empty()
            && let Some((_, row)) = self.group_by_row.pop_first()
        {
            self.pending.extend(row);
        }
        if !self.pending.is_empty() {
            for _ in 0..t.rng.randint(1, 2) {
                if self.pending.is_empty() {
                    break;
                }
                let index = t.rng.randint(0, self.pending.len() as i64 - 1) as usize;
                let id = self.pending.remove(index);
                t.set_visible(id, true);
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
