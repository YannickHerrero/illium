//! `expand`: characters expand from the center.
use crate::engine::*;

const MOVEMENT_SPEED: f64 = 0.35;
const EXPAND_EASING: Ease = Ease::InOutQuart;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Expand {
    active: Active,
}

impl Expand {
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
        let center = t.canvas.center;
        let mut active = Active::default();
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(center);
            let path = ch.motion.path(MOVEMENT_SPEED, Some(EXPAND_EASING));
            let input_coord = ch.input_coord;
            ch.motion.get(path).waypoint(input_coord);
            ch.register(
                Event::PathActivated,
                Caller::Path(path),
                Action::SetLayer(1),
            );
            ch.register(Event::PathComplete, Caller::Path(path), Action::SetLayer(0));
            t.set_visible(id, true);
            active.add(id);
            t.activate_path(id, path);
            let ch = &mut t.chars[id];
            let scene = ch
                .animation
                .new_scene(false, Some(SyncMetric::Distance), None, "");
            let gradient = Gradient::new(&[final_gradient[0], mapping[&input_coord]], &[10]);
            let symbol = ch.input_symbol;
            ch.animation
                .get(scene)
                .apply_gradient_to_symbols(&[symbol], 5, Some(&gradient), None);
            t.activate_scene(id, scene);
        }
        Self { active }
    }
}

impl Effect for Expand {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() {
            return false;
        }
        self.active.update(t);
        true
    }
}
