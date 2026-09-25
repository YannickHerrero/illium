//! `wipe`: wipes the text across the terminal to reveal characters.
use crate::engine::*;

const WIPE_DIRECTION: CharacterGroup = CharacterGroup::DiagonalTopLeftToBottomRight;
const WIPE_DELAY: usize = 0;
const WIPE_EASE: Ease = Ease::InOutCirc;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["833ab4", "fd1d1d", "fcb045"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 3;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Wipe {
    easer: SequenceEaser<Vec<CharId>>,
    wipe_delay: usize,
    active: Active,
}

impl Wipe {
    pub fn new(t: &mut Terminal) -> Self {
        let easer = SequenceEaser::new(
            t.get_characters_grouped(WIPE_DIRECTION, Select::INPUT),
            WIPE_EASE,
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
            let wipe_gradient = Gradient::new(
                &[final_gradient[0], mapping[&ch.input_coord]],
                &[FINAL_GRADIENT_STEPS],
            );
            let symbol = ch.input_symbol;
            let scene = ch.animation.named_scene("wipe");
            ch.animation.get(scene).apply_gradient_to_symbols(
                &[symbol],
                FINAL_GRADIENT_FRAMES,
                Some(&wipe_gradient),
                None,
            );
        }
        Self {
            easer,
            wipe_delay: WIPE_DELAY,
            active: Active::default(),
        }
    }
}

impl Effect for Wipe {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && self.easer.is_complete() {
            return false;
        }
        if self.wipe_delay == 0 {
            self.easer.step();
            for group in &self.easer.added {
                for &id in group {
                    let scene = t.chars[id].animation.query_scene("wipe");
                    t.activate_scene(id, scene);
                    t.set_visible(id, true);
                    self.active.add(id);
                }
            }
            for group in &self.easer.removed {
                for &id in group {
                    let animation = &mut t.chars[id].animation;
                    animation.deactivate_scene(None);
                    let scene = animation.query_scene("wipe");
                    animation.get(scene).reset();
                    t.set_visible(id, false);
                }
            }
            self.wipe_delay = WIPE_DELAY;
        } else {
            self.wipe_delay -= 1;
        }
        self.active.update(t);
        true
    }
}
