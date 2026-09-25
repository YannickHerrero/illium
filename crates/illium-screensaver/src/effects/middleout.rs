//! `middleout`: text expands in a single row or column in the middle of the
//! canvas, then out.
use crate::engine::*;

const STARTING_COLOR: &str = "ffffff";
const EXPAND_VERTICAL: bool = true;
const CENTER_MOVEMENT_SPEED: f64 = 0.6;
const FULL_MOVEMENT_SPEED: f64 = 0.6;
const CENTER_EASING: Ease = Ease::InOutSine;
const FULL_EASING: Ease = Ease::InOutSine;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

enum Phase {
    Center,
    Full,
}

pub struct MiddleOut {
    active: Active,
    phase: Phase,
}

impl MiddleOut {
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
        let starting_color = Color::hex(STARTING_COLOR);
        let (center, center_row, center_column) =
            (t.canvas.center, t.canvas.center_row, t.canvas.center_column);
        let mut active = Active::default();
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(center);
            let input_coord = ch.input_coord;
            let target = if EXPAND_VERTICAL {
                Coord::new(input_coord.column, center_row)
            } else {
                Coord::new(center_column, input_coord.row)
            };
            let center_path = ch.motion.path(CENTER_MOVEMENT_SPEED, Some(CENTER_EASING));
            ch.motion.get(center_path).waypoint(target);
            let full_path = ch.motion.new_path(
                FULL_MOVEMENT_SPEED,
                Some(FULL_EASING),
                None,
                0,
                false,
                "full",
            );
            ch.motion
                .get(full_path)
                .new_waypoint(input_coord, &[], "full");
            let full_scene = ch.animation.named_scene("full");
            let gradient = Gradient::new(&[starting_color, mapping[&input_coord]], &[10]);
            let symbol = ch.input_symbol;
            ch.animation.get(full_scene).apply_gradient_to_symbols(
                &[symbol],
                6,
                Some(&gradient),
                None,
            );
            t.activate_path(id, center_path);
            t.chars[id]
                .animation
                .set_appearance(Some(symbol), ColorPair::fg(starting_color));
            t.set_visible(id, true);
            active.add(id);
        }
        Self {
            active,
            phase: Phase::Center,
        }
    }
}

impl Effect for MiddleOut {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if matches!(self.phase, Phase::Center) && self.active.is_empty() {
            self.phase = Phase::Full;
            for id in t.input_characters() {
                self.active.add(id);
                let path = t.chars[id].motion.query_path("full");
                t.activate_path(id, path);
                let scene = t.chars[id].animation.query_scene("full");
                t.activate_scene(id, scene);
            }
        }
        if self.active.is_empty() {
            return false;
        }
        self.active.update(t);
        true
    }
}
