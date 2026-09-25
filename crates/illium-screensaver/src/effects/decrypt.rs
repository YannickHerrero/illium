//! `decrypt`: movie style decryption effect.
use crate::engine::*;
use std::collections::HashMap;

const TYPING_SPEED: usize = 2;
const CIPHERTEXT_COLORS: [&str; 3] = ["008000", "00cb00", "00ff00"];
const FINAL_GRADIENT_STOPS: [&str; 1] = ["eda000"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

enum Phase {
    Typing,
    Decrypting,
}

pub struct Decrypt {
    typing_pending: Vec<CharId>,
    decrypting_pending: Active,
    active: Active,
    phase: Phase,
    typing: HashMap<CharId, SceneId>,
    fast_decrypt: HashMap<CharId, SceneId>,
}

fn encrypted_symbols() -> Vec<char> {
    (33..127)
        .chain(9608..9632)
        .chain(9472..9599)
        .chain(174..452)
        .filter_map(char::from_u32)
        .collect()
}

impl Decrypt {
    pub fn new(t: &mut Terminal) -> Self {
        let ciphertext = colors(&CIPHERTEXT_COLORS);
        let symbols = encrypted_symbols();
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
        let final_colors: HashMap<CharId, Color> = chars
            .iter()
            .map(|&id| (id, mapping[&t.chars[id].input_coord]))
            .collect();
        let mut effect = Self {
            typing_pending: vec![],
            decrypting_pending: Active::default(),
            active: Active::default(),
            phase: Phase::Typing,
            typing: HashMap::new(),
            fast_decrypt: HashMap::new(),
        };
        // prepare_data_for_type_effect
        for &id in &chars {
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let scene = ch.animation.named_scene("typing");
            for block in ['▉', '▓', '▒', '░'] {
                let color = *rng.choice(&ciphertext);
                ch.animation
                    .get(scene)
                    .add_frame(block, 2, ColorPair::fg(color));
            }
            let symbol = *rng.choice(&symbols);
            let color = *rng.choice(&ciphertext);
            ch.animation
                .get(scene)
                .add_frame(symbol, 1, ColorPair::fg(color));
            effect.typing.insert(id, scene);
            effect.typing_pending.push(id);
        }
        // prepare_data_for_decrypt_effect
        for &id in &chars {
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let fast = ch.animation.named_scene("fast_decrypt");
            let color = *rng.choice(&ciphertext);
            for _ in 0..80 {
                let symbol = *rng.choice(&symbols);
                ch.animation
                    .get(fast)
                    .add_frame(symbol, 2, ColorPair::fg(color));
            }
            let slow = ch.animation.named_scene("slow_decrypt");
            for _ in 0..rng.randint(1, 15) {
                let symbol = *rng.choice(&symbols);
                let duration = if rng.randint(0, 100) <= 30 {
                    rng.randrange(35, 60)
                } else {
                    rng.randrange(3, 6)
                };
                ch.animation
                    .get(slow)
                    .add_frame(symbol, duration as usize, ColorPair::fg(color));
            }
            let discovered = ch.animation.named_scene("discovered");
            let gradient = Gradient::new(&[Color::hex("ffffff"), final_colors[&id]], &[10]);
            let symbol = ch.input_symbol;
            ch.animation.get(discovered).apply_gradient_to_symbols(
                &[symbol],
                5,
                Some(&gradient),
                None,
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(fast),
                Action::ActivateScene(slow),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(slow),
                Action::ActivateScene(discovered),
            );
            t.activate_scene(id, fast);
            effect.fast_decrypt.insert(id, fast);
            effect.decrypting_pending.add(id);
        }
        effect
    }
}

impl Effect for Decrypt {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if let Phase::Typing = self.phase {
            if !self.typing_pending.is_empty() || !self.active.is_empty() {
                if !self.typing_pending.is_empty() && t.rng.randint(0, 100) <= 75 {
                    for _ in 0..TYPING_SPEED {
                        if !self.typing_pending.is_empty() {
                            let id = self.typing_pending.remove(0);
                            t.set_visible(id, true);
                            t.activate_scene(id, self.typing[&id]);
                            self.active.add(id);
                        }
                    }
                }
                self.active.update(t);
                return true;
            }
            self.active = std::mem::take(&mut self.decrypting_pending);
            for &id in &self.active.0 {
                t.activate_scene(id, self.fast_decrypt[&id]);
            }
            self.phase = Phase::Decrypting;
        }
        if !self.active.is_empty() {
            self.active.update(t);
            return true;
        }
        false
    }
}
