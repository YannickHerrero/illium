//! `waves`: waves travel across the terminal leaving behind the characters
//! (the default left to right column direction).
use crate::engine::*;

const WAVE_SYMBOLS: [char; 15] = [
    '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '▇', '▆', '▅', '▄', '▃', '▂', '▁',
];
const WAVE_GRADIENT_STOPS: [&str; 5] = ["f0ff65", "ffb102", "31a0d4", "ffb102", "f0ff65"];
const WAVE_GRADIENT_STEPS: [usize; 1] = [6];
const WAVE_COUNT: usize = 7;
const WAVE_LENGTH: usize = 2;
const WAVE_EASING: Ease = Ease::InOutSine;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["ffb102", "31a0d4", "f0ff65"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

pub struct Waves {
    pending_columns: Vec<Vec<CharId>>,
    active: Active,
}

impl Waves {
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
        let wave_gradient = Gradient::new(&colors(&WAVE_GRADIENT_STOPS), &WAVE_GRADIENT_STEPS);
        let wave_end = *wave_gradient.spectrum.last().unwrap();
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            let wave = ch.animation.new_scene(false, None, Some(WAVE_EASING), "");
            for _ in 0..WAVE_COUNT {
                ch.animation.get(wave).apply_gradient_to_symbols(
                    &WAVE_SYMBOLS,
                    WAVE_LENGTH,
                    Some(&wave_gradient),
                    None,
                );
            }
            let last = ch.animation.scene();
            let symbol = ch.input_symbol;
            let gradient = Gradient::new(
                &[wave_end, mapping[&ch.input_coord]],
                &[FINAL_GRADIENT_STEPS],
            );
            for &step in &gradient.spectrum {
                ch.animation
                    .get(last)
                    .add_frame(symbol, 10, ColorPair::fg(step));
            }
            ch.register(
                Event::SceneComplete,
                Caller::Scene(wave),
                Action::ActivateScene(last),
            );
            t.activate_scene(id, wave);
        }
        let pending_columns =
            t.get_characters_grouped(CharacterGroup::ColumnLeftToRight, Select::INPUT);
        Self {
            pending_columns,
            active: Active::default(),
        }
    }
}

impl Effect for Waves {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_columns.is_empty() && self.active.is_empty() {
            return false;
        }
        if !self.pending_columns.is_empty() {
            for id in self.pending_columns.remove(0) {
                t.set_visible(id, true);
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
