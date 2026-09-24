//! `colorshift`: a gradient that shifts colors across the terminal.
use crate::engine::geometry::find_normalized_distance_from_center;
use crate::engine::*;
use std::collections::HashMap;

const GRADIENT_STOPS: [&str; 7] = [
    "e81416", "ffa500", "faeb36", "79c314", "487de7", "4b369d", "70369d",
];
const GRADIENT_STEPS: usize = 12;
const GRADIENT_FRAMES: usize = 2;
const TRAVEL_DIRECTION: Direction = Direction::Radial;
const CYCLES: usize = 3;
const FINAL_GRADIENT_STOPS: [&str; 7] = GRADIENT_STOPS;
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

const LOOP_TRACKER: u32 = 0;

pub struct ColorShift {
    active: Active,
    loop_tracker_map: HashMap<CharId, usize>,
}

impl ColorShift {
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
        let gradient = Gradient::looped(&colors(&GRADIENT_STOPS), &[GRADIENT_STEPS]);
        let mut active = Active::default();
        for id in t.input_characters() {
            t.set_visible(id, true);
            let c = &t.canvas;
            let coord = t.chars[id].input_coord;
            let direction_index = match TRAVEL_DIRECTION {
                Direction::Horizontal => coord.column as f64 / c.right as f64,
                Direction::Vertical => coord.row as f64 / c.top as f64,
                Direction::Diagonal => (coord.row + coord.column) as f64 / (c.right + c.top) as f64,
                Direction::Radial => find_normalized_distance_from_center(
                    c.text_bottom,
                    c.text_top,
                    c.text_left,
                    c.text_right,
                    coord,
                ),
            };
            let shift_distance = (gradient.len() as f64 * direction_index) as usize;
            let mut shifted = gradient.spectrum.clone();
            // Python's `spectrum[n:] + spectrum[:n]` leaves the list unchanged past its end.
            if shift_distance < shifted.len() {
                shifted.rotate_left(shift_distance);
            }
            let final_color = mapping[&coord];
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            let gradient_scene = ch.animation.named_scene("gradient");
            for &color in &shifted {
                ch.animation.get(gradient_scene).add_frame(
                    symbol,
                    GRADIENT_FRAMES,
                    ColorPair::fg(color),
                );
            }
            let final_scene = ch.animation.named_scene("final_gradient");
            for &color in &Gradient::new(&[*shifted.last().unwrap(), final_color], &[8]).spectrum {
                ch.animation.get(final_scene).add_frame(
                    symbol,
                    GRADIENT_FRAMES,
                    ColorPair::fg(color),
                );
            }
            t.activate_scene(id, gradient_scene);
            active.add(id);
            t.chars[id].register(
                Event::SceneComplete,
                Caller::Scene(gradient_scene),
                Action::Callback(LOOP_TRACKER, 0),
            );
        }
        Self {
            active,
            loop_tracker_map: HashMap::new(),
        }
    }
}

impl Effect for ColorShift {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() {
            return false;
        }
        self.active.update(t);
        for (id, tag, _) in t.take_callbacks() {
            if tag != LOOP_TRACKER {
                continue;
            }
            let count = self.loop_tracker_map.entry(id).or_insert(0);
            *count += 1;
            let scene = if *count < CYCLES {
                t.chars[id].animation.query_scene("gradient")
            } else {
                t.chars[id].animation.query_scene("final_gradient")
            };
            t.activate_scene(id, scene);
            self.active.add(id);
        }
        true
    }
}
