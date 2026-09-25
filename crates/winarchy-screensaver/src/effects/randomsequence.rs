//! `randomsequence`: prints the input data in a random sequence.
use crate::engine::*;

const SPEED: f64 = 0.007;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 8;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct RandomSequence {
    pending: Vec<CharId>,
    active: Active,
    characters_per_tick: usize,
}

impl RandomSequence {
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
        let chars = t.input_characters();
        let characters_per_tick = ((SPEED * chars.len() as f64) as usize).max(1);
        let mut pending = vec![];
        for &id in &chars {
            let background = t.background;
            let ch = &mut t.chars[id];
            ch.visible = false;
            let scene = ch.animation.scene();
            let gradient = Gradient::new(&[background, mapping[&ch.input_coord]], &[7]);
            let symbol = ch.input_symbol;
            ch.animation.get(scene).apply_gradient_to_symbols(
                &[symbol],
                FINAL_GRADIENT_FRAMES,
                Some(&gradient),
                None,
            );
            t.activate_scene(id, scene);
            pending.push(id);
        }
        t.rng.shuffle(&mut pending);
        Self {
            pending,
            active: Active::default(),
            characters_per_tick,
        }
    }
}

impl Effect for RandomSequence {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending.is_empty() && self.active.is_empty() {
            return false;
        }
        for _ in 0..self.characters_per_tick {
            if let Some(id) = self.pending.pop() {
                t.set_visible(id, true);
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
