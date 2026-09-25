//! `spray`: draws the characters spawning at varying rates from a single
//! point (the default east position).
use crate::engine::*;

const SPRAY_VOLUME: f64 = 0.005;
const MOVEMENT_SPEED_RANGE: (f64, f64) = (0.6, 1.4);
const MOVEMENT_EASING: Ease = Ease::OutExpo;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Spray {
    pending: Vec<CharId>,
    active: Active,
    volume: i64,
}

impl Spray {
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
        let origin = Coord::new(t.canvas.right - 1, t.canvas.top / 2);
        let mut pending = vec![];
        for id in t.input_characters() {
            let speed = t
                .rng
                .uniform(MOVEMENT_SPEED_RANGE.0, MOVEMENT_SPEED_RANGE.1);
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(origin);
            let path = ch.motion.path(speed, Some(MOVEMENT_EASING));
            let input_coord = ch.input_coord;
            ch.motion.get(path).waypoint(input_coord);
            ch.register(
                Event::PathActivated,
                Caller::Path(path),
                Action::SetLayer(1),
            );
            ch.register(Event::PathComplete, Caller::Path(path), Action::SetLayer(0));
            let scene = ch.animation.scene();
            let start = *t.rng.choice(&final_gradient.spectrum);
            let ch = &mut t.chars[id];
            let gradient = Gradient::new(&[start, mapping[&input_coord]], &[7]);
            let symbol = ch.input_symbol;
            ch.animation
                .get(scene)
                .apply_gradient_to_symbols(&[symbol], 20, Some(&gradient), None);
            t.activate_scene(id, scene);
            t.activate_path(id, path);
            pending.push(id);
        }
        t.rng.shuffle(&mut pending);
        let volume = ((pending.len() as f64 * SPRAY_VOLUME) as i64).max(1);
        Self {
            pending,
            active: Active::default(),
            volume,
        }
    }
}

impl Effect for Spray {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending.is_empty() && self.active.is_empty() {
            return false;
        }
        if !self.pending.is_empty() {
            for _ in 0..t.rng.randint(1, self.volume) {
                if let Some(id) = self.pending.pop() {
                    t.set_visible(id, true);
                    self.active.add(id);
                }
            }
        }
        self.active.update(t);
        true
    }
}
