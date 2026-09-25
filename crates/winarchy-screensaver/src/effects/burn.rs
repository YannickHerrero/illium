//! `burn`: characters are ignited and burn up the screen.
use crate::engine::spanningtree::PrimsSimple;
use crate::engine::*;
use std::collections::{HashMap, VecDeque};

const STARTING_COLOR: &str = "837373";
const BURN_COLORS: [&str; 5] = ["ffffff", "fff75d", "fe650d", "8a003c", "510100"];
const SMOKE_CHANCE: f64 = 0.5;
const FINAL_GRADIENT_STOPS: [&str; 2] = ["00c3ff", "ffff1c"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

const EMIT_SMOKE: u32 = 0;

pub struct Burn {
    link_order: VecDeque<CharId>,
    smoke_particles: VecDeque<CharId>,
    pending_smoke: Active,
    active: Active,
    burn: HashMap<CharId, SceneId>,
}

impl Burn {
    pub fn new(t: &mut Terminal) -> Self {
        let algo = PrimsSimple::new(t, None, true);
        let mut effect = Self {
            link_order: VecDeque::new(),
            smoke_particles: VecDeque::new(),
            pending_smoke: Active::default(),
            active: Active::default(),
            burn: HashMap::new(),
        };
        effect.make_smoke(t);
        effect.build(t, algo);
        effect
    }

    fn make_smoke(&mut self, t: &mut Terminal) {
        let gradient = Gradient::new(&colors(&["504f4f", "c7c7c7"]), &[9]);
        for _ in 0..2000 {
            let symbol = *t.rng.choice(&['.', ',', '\'', '`', '#', '*']);
            let id = t.add_character(symbol, Coord::new(0, 0));
            let ch = &mut t.chars[id];
            let smoke = ch.animation.named_scene("smoke");
            for &color in &gradient.spectrum {
                ch.animation
                    .get(smoke)
                    .add_frame(symbol, 10, ColorPair::fg(color));
            }
            ch.register(Event::SceneComplete, Caller::Scene(smoke), Action::Hide);
            ch.layer = 2;
            self.smoke_particles.push_back(id);
        }
    }

    fn emit_smoke(&mut self, t: &mut Terminal, origin: Coord) {
        if t.rng.random() > SMOKE_CHANCE {
            return;
        }
        let id = *self.smoke_particles.back().unwrap();
        self.smoke_particles.rotate_right(1);
        let target = Coord::new(
            t.rng
                .randint(origin.column as i64 - 4, origin.column as i64 + 4) as i32,
            t.canvas.top + 1,
        );
        let ch = &mut t.chars[id];
        ch.motion.set_coordinate(origin);
        if let Some(scene) = ch.animation.active {
            ch.animation.get(scene).reset();
        }
        ch.visible = true;
        let path = ch.motion.path(0.5, None);
        ch.motion.get(path).waypoint(target);
        let smoke = ch.animation.query_scene("smoke");
        t.activate_path(id, path);
        t.activate_scene(id, smoke);
        self.pending_smoke.add(id);
    }

    fn build(&mut self, t: &mut Terminal, mut algo: PrimsSimple) {
        let burn_char_order = ['\'', '.', '▖', '▙', '█', '▜', '▀', '▝', '.'];
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let fire_gradient = Gradient::new(&colors(&BURN_COLORS), &[10]);
        while !algo.complete {
            algo.step(t);
        }
        self.link_order = algo.char_link_order.into();
        let starting_color = Color::hex(STARTING_COLOR);
        let fire_end = *fire_gradient.spectrum.last().unwrap();
        for id in t.input_characters() {
            let final_color = mapping[&t.chars[id].input_coord];
            let ch = &mut t.chars[id];
            ch.visible = true;
            let symbol = ch.input_symbol;
            ch.animation
                .set_appearance(Some(symbol), ColorPair::fg(starting_color));
            let burn = ch.animation.named_scene("burn");
            ch.animation.get(burn).apply_gradient_to_symbols(
                &burn_char_order,
                4,
                Some(&fire_gradient),
                None,
            );
            let final_scene = ch.animation.scene();
            for &color in &Gradient::new(&[fire_end, final_color], &[8]).spectrum {
                ch.animation
                    .get(final_scene)
                    .add_frame(symbol, 4, ColorPair::fg(color));
            }
            ch.register(
                Event::SceneComplete,
                Caller::Scene(burn),
                Action::ActivateScene(final_scene),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(burn),
                Action::Callback(EMIT_SMOKE, 0),
            );
            self.burn.insert(id, burn);
        }
    }
}

impl Effect for Burn {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.link_order.is_empty() && self.active.is_empty() {
            return false;
        }
        for id in std::mem::take(&mut self.pending_smoke).0 {
            self.active.add(id);
        }
        for _ in 0..t.rng.randint(2, 4) {
            let Some(id) = self.link_order.pop_front() else {
                continue;
            };
            if t.chars[id].input_symbol == ' ' {
                continue;
            }
            t.activate_scene(id, self.burn[&id]);
            self.active.add(id);
        }
        self.active.update(t);
        for (id, tag, _) in t.take_callbacks() {
            if tag == EMIT_SMOKE {
                let origin = t.chars[id].input_coord;
                self.emit_smoke(t, origin);
            }
        }
        true
    }
}
