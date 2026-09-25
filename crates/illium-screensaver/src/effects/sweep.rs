//! `sweep`: sweeps across the canvas to reveal uncolored text, then sweeps
//! back to color it.
use crate::engine::*;

const SWEEP_SYMBOLS: [char; 4] = ['█', '▓', '▒', '░'];
const FIRST_SWEEP_DIRECTION: CharacterGroup = CharacterGroup::ColumnRightToLeft;
const SECOND_SWEEP_DIRECTION: CharacterGroup = CharacterGroup::ColumnLeftToRight;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 8;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const SHADES_OF_GRAY: [&str; 5] = ["A0A0A0", "808080", "404040", "202020", "101010"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    FirstSweep,
    SecondSweep,
}

pub struct Sweep {
    easer: SequenceEaser<Vec<CharId>>,
    groups_second_sweep: Vec<Vec<CharId>>,
    active: Active,
    phase: Phase,
    complete: bool,
}

impl Sweep {
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
        let grays = colors(&SHADES_OF_GRAY);
        for id in t.get_characters(Select::ALL_CELLS, CharacterSort::TopToBottomLeftToRight) {
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let final_colors = if ch.is_fill {
                ColorPair::fg(Color::hex("000000"))
            } else {
                ColorPair::fg(mapping[&ch.input_coord])
            };
            let symbol = ch.input_symbol;
            let initial = ch.animation.named_scene("initial_sweep");
            for sweep_symbol in SWEEP_SYMBOLS {
                let color = *rng.choice(&grays);
                ch.animation
                    .get(initial)
                    .add_frame(sweep_symbol, 5, ColorPair::fg(color));
            }
            ch.animation
                .get(initial)
                .add_frame(symbol, 1, ColorPair::fg(Color::hex("808080")));
            let second = ch.animation.named_scene("second_sweep");
            for sweep_symbol in SWEEP_SYMBOLS {
                let color = *rng.choice(&final_gradient.spectrum);
                ch.animation
                    .get(second)
                    .add_frame(sweep_symbol, 5, ColorPair::fg(color));
            }
            ch.animation.get(second).add_frame(symbol, 1, final_colors);
        }
        let groups_first_sweep = t.get_characters_grouped(FIRST_SWEEP_DIRECTION, Select::ALL_CELLS);
        let groups_second_sweep =
            t.get_characters_grouped(SECOND_SWEEP_DIRECTION, Select::ALL_CELLS);
        Self {
            easer: SequenceEaser::new(groups_first_sweep, Ease::InOutCirc, 100),
            groups_second_sweep,
            active: Active::default(),
            phase: Phase::FirstSweep,
            complete: false,
        }
    }
}

impl Effect for Sweep {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && self.complete {
            return false;
        }
        self.easer.step();
        let scene = match self.phase {
            Phase::FirstSweep => "initial_sweep",
            Phase::SecondSweep => "second_sweep",
        };
        for group in &self.easer.added {
            for &id in group {
                if self.phase == Phase::FirstSweep {
                    t.set_visible(id, true);
                }
                let scene = t.chars[id].animation.query_scene(scene);
                t.activate_scene(id, scene);
            }
            for &id in group {
                self.active.add(id);
            }
        }
        if self.easer.is_complete() {
            match self.phase {
                Phase::FirstSweep => {
                    self.easer.sequence = std::mem::take(&mut self.groups_second_sweep);
                    self.easer.reset();
                    self.phase = Phase::SecondSweep;
                }
                Phase::SecondSweep => self.complete = true,
            }
        }
        self.active.update(t);
        true
    }
}
