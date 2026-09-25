//! `scattered`: text is scattered across the canvas and moves into position.
use crate::engine::*;

const MOVEMENT_SPEED: f64 = 0.5;
const MOVEMENT_EASING: Ease = Ease::InOutBack;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["ff9048", "ab9dff", "bdffea"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 9;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const INITIAL_HOLD_FRAMES: usize = 25;

pub struct Scattered {
    active: Active,
    initial_hold_frames: usize,
}

impl Scattered {
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
        let mut active = Active::default();
        for id in t.input_characters() {
            let start = if t.canvas.right < 2 || t.canvas.top < 2 {
                Coord::new(1, 1)
            } else {
                t.canvas.random_coord(&mut t.rng, false, false)
            };
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(start);
            let path = ch.motion.path(MOVEMENT_SPEED, Some(MOVEMENT_EASING));
            let input_coord = ch.input_coord;
            ch.motion.get(path).waypoint(input_coord);
            ch.register(
                Event::PathActivated,
                Caller::Path(path),
                Action::SetLayer(1),
            );
            ch.register(Event::PathComplete, Caller::Path(path), Action::SetLayer(0));
            t.activate_path(id, path);
            t.set_visible(id, true);
            let ch = &mut t.chars[id];
            let scene = ch
                .animation
                .new_scene(false, Some(SyncMetric::Distance), None, "");
            let gradient = Gradient::new(&[final_gradient[0], mapping[&input_coord]], &[10]);
            let symbol = ch.input_symbol;
            ch.animation.get(scene).apply_gradient_to_symbols(
                &[symbol],
                FINAL_GRADIENT_FRAMES,
                Some(&gradient),
                None,
            );
            t.activate_scene(id, scene);
            active.add(id);
        }
        Self {
            active,
            initial_hold_frames: INITIAL_HOLD_FRAMES,
        }
    }
}

impl Effect for Scattered {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() {
            return false;
        }
        if self.initial_hold_frames > 0 {
            self.initial_hold_frames -= 1;
            return true;
        }
        self.active.update(t);
        true
    }
}
