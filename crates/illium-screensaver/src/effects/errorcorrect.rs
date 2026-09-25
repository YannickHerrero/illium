//! `errorcorrect`: some characters start in the wrong position and are
//! corrected in sequence.
use crate::engine::*;

const ERROR_PAIRS: f64 = 0.1;
const SWAP_DELAY: usize = 6;
const ERROR_COLOR: &str = "e74c3c";
const CORRECT_COLOR: &str = "45bf55";
const MOVEMENT_SPEED: f64 = 0.9;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const BLOCK_WIPE_START: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const BLOCK_WIPE_END: [char; 7] = ['▇', '▆', '▅', '▄', '▃', '▂', '▁'];

pub struct ErrorCorrect {
    swapped: Vec<(CharId, CharId)>,
    swap_delay: usize,
    active: Active,
}

fn configure_swapped_character(
    t: &mut Terminal,
    id: CharId,
    final_color: Color,
    correcting_gradient: &Gradient,
) {
    let error_color = Color::hex(ERROR_COLOR);
    let correct_color = Color::hex(CORRECT_COLOR);
    let ch = &mut t.chars[id];
    let symbol = ch.input_symbol;
    let first_block_wipe = ch.animation.scene();
    let last_block_wipe = ch.animation.scene();
    for block in BLOCK_WIPE_START {
        ch.animation
            .get(first_block_wipe)
            .add_frame(block, 3, ColorPair::fg(error_color));
    }
    for block in BLOCK_WIPE_END {
        ch.animation
            .get(last_block_wipe)
            .add_frame(block, 3, ColorPair::fg(correct_color));
    }
    let initial = ch.animation.scene();
    ch.animation
        .get(initial)
        .add_frame(symbol, 1, ColorPair::fg(error_color));
    t.activate_scene(id, initial);
    let ch = &mut t.chars[id];
    let error = ch.animation.named_scene("error");
    for _ in 0..10 {
        ch.animation
            .get(error)
            .add_frame('▓', 3, ColorPair::fg(error_color));
        ch.animation
            .get(error)
            .add_frame(symbol, 3, ColorPair::fg(Color::hex("ffffff")));
    }
    let correcting = ch
        .animation
        .new_scene(false, Some(SyncMetric::Distance), None, "");
    ch.animation.get(correcting).apply_gradient_to_symbols(
        &['█'],
        3,
        Some(correcting_gradient),
        None,
    );
    let final_scene = ch.animation.scene();
    let final_gradient = Gradient::new(&[correct_color, final_color], &[10]);
    ch.animation.get(final_scene).apply_gradient_to_symbols(
        &[symbol],
        3,
        Some(&final_gradient),
        None,
    );
    let path = ch.motion.query_path("input_coord");
    ch.register(
        Event::SceneComplete,
        Caller::Scene(error),
        Action::ActivateScene(first_block_wipe),
    );
    ch.register(
        Event::SceneComplete,
        Caller::Scene(first_block_wipe),
        Action::ActivateScene(correcting),
    );
    ch.register(
        Event::SceneComplete,
        Caller::Scene(first_block_wipe),
        Action::ActivatePath(path),
    );
    ch.register(
        Event::PathActivated,
        Caller::Path(path),
        Action::SetLayer(1),
    );
    ch.register(Event::PathComplete, Caller::Path(path), Action::SetLayer(0));
    ch.register(
        Event::PathComplete,
        Caller::Path(path),
        Action::ActivateScene(last_block_wipe),
    );
    ch.register(
        Event::SceneComplete,
        Caller::Scene(last_block_wipe),
        Action::ActivateScene(final_scene),
    );
}

impl ErrorCorrect {
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
        for &id in &chars {
            let ch = &mut t.chars[id];
            let spawn = ch.animation.scene();
            let symbol = ch.input_symbol;
            let color = mapping[&ch.input_coord];
            ch.animation
                .get(spawn)
                .add_frame(symbol, 1, ColorPair::fg(color));
            t.activate_scene(id, spawn);
            t.set_visible(id, true);
        }
        let mut all_characters = chars.clone();
        let correcting_gradient =
            Gradient::new(&[Color::hex(ERROR_COLOR), Color::hex(CORRECT_COLOR)], &[10]);
        let mut swapped = vec![];
        for _ in 0..(ERROR_PAIRS * chars.len() as f64) as usize {
            if all_characters.len() < 2 {
                break;
            }
            let index = t.rng.randrange(0, all_characters.len() as i64) as usize;
            let char1 = all_characters.remove(index);
            let index = t.rng.randrange(0, all_characters.len() as i64) as usize;
            let char2 = all_characters.remove(index);
            let (coord1, coord2) = (t.chars[char1].input_coord, t.chars[char2].input_coord);
            for (id, from, to) in [(char1, coord2, coord1), (char2, coord1, coord2)] {
                let ch = &mut t.chars[id];
                ch.motion.set_coordinate(from);
                let path = ch
                    .motion
                    .new_path(MOVEMENT_SPEED, None, None, 0, false, "input_coord");
                ch.motion.get(path).waypoint(to);
            }
            swapped.push((char1, char2));
            for id in [char1, char2] {
                let final_color = mapping[&t.chars[id].input_coord];
                configure_swapped_character(t, id, final_color, &correcting_gradient);
            }
        }
        Self {
            swapped,
            swap_delay: 0,
            active: Active::default(),
        }
    }
}

impl Effect for ErrorCorrect {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if !self.swapped.is_empty() && self.swap_delay == 0 {
            let (char1, char2) = self.swapped.remove(0);
            for id in [char1, char2] {
                let scene = t.chars[id].animation.query_scene("error");
                t.activate_scene(id, scene);
                self.active.add(id);
            }
            self.swap_delay = SWAP_DELAY;
        } else if self.swap_delay > 0 {
            self.swap_delay -= 1;
        }
        if self.active.is_empty() {
            return false;
        }
        self.active.update(t);
        true
    }
}
