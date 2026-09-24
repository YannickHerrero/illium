//! `highlight`: runs a specular highlight across the text.
use crate::engine::*;

const HIGHLIGHT_BRIGHTNESS: f64 = 1.75;
const HIGHLIGHT_DIRECTION: CharacterGroup = CharacterGroup::DiagonalBottomLeftToTopRight;
const HIGHLIGHT_WIDTH: usize = 8;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Highlight {
    easer: SequenceEaser<Vec<CharId>>,
    active: Active,
}

impl Highlight {
    pub fn new(t: &mut Terminal) -> Self {
        let easer = SequenceEaser::new(
            t.get_characters_grouped(HIGHLIGHT_DIRECTION, Select::INPUT),
            Ease::InOutCirc,
            100,
        );
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
            let base_color = mapping[&ch.input_coord];
            let highlight_color = adjust_color_brightness(base_color, HIGHLIGHT_BRIGHTNESS);
            let highlight_gradient = Gradient::new(
                &[base_color, highlight_color, highlight_color, base_color],
                &[3, HIGHLIGHT_WIDTH, 3],
            );
            let symbol = ch.input_symbol;
            ch.animation
                .set_appearance(Some(symbol), ColorPair::fg(base_color));
            let scene = ch.animation.named_scene("highlight");
            for &color in &highlight_gradient.spectrum {
                ch.animation
                    .get(scene)
                    .add_frame(symbol, 2, ColorPair::fg(color));
            }
            t.set_visible(id, true);
        }
        Self {
            easer,
            active: Active::default(),
        }
    }
}

impl Effect for Highlight {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && self.easer.is_complete() {
            return false;
        }
        self.easer.step();
        for group in &self.easer.added {
            for &id in group {
                let scene = t.chars[id].animation.query_scene("highlight");
                t.activate_scene(id, scene);
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
