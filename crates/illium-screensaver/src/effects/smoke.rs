//! `smoke`: smoke floods the canvas colorizing any characters it crosses.
use crate::engine::spanningtree::{BreadthFirst, PrimsWeighted};
use crate::engine::*;
use std::collections::HashMap;

const STARTING_COLOR: &str = "7a7a7a";
const SMOKE_SYMBOLS: [char; 5] = ['░', '▒', '▓', '▒', '░'];
const SMOKE_GRADIENT_STOPS: [&str; 2] = ["242424", "ffffff"];
const USE_WHOLE_CANVAS: bool = false;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Smoke {
    fill_alg: BreadthFirst,
    smoke: HashMap<CharId, SceneId>,
    active: Active,
}

impl Smoke {
    pub fn new(t: &mut Terminal) -> Self {
        let limit = !USE_WHOLE_CANVAS;
        let mut gen_alg = PrimsWeighted::new(t, None, limit);
        let coord = t.canvas.random_coord(&mut t.rng, false, limit);
        let fill_start = t.get_character_by_input_coord(coord);
        let final_stops = colors(&FINAL_GRADIENT_STOPS);
        let final_gradient = Gradient::new(&final_stops, &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let black = Color::hex("000000");
        let mut smoke_stops = colors(&SMOKE_GRADIENT_STOPS);
        smoke_stops.extend(final_stops.iter().rev());
        let smoke_gradient = Gradient::new(&smoke_stops, &[3, 4]);
        let base_colors = ColorPair::fg(Color::hex(STARTING_COLOR));
        let mut smoke = HashMap::new();
        for id in t.get_characters(Select::ALL_CELLS, CharacterSort::TopToBottomLeftToRight) {
            let final_color = mapping
                .get(&t.chars[id].input_coord)
                .copied()
                .unwrap_or(black);
            let ch = &mut t.chars[id];
            ch.visible = true;
            let symbol = ch.input_symbol;
            let paint = ch.animation.named_scene("paint");
            let mut paint_stops = final_stops.clone();
            paint_stops.push(final_color);
            let paint_gradient = Gradient::new(&paint_stops, &[5]);
            ch.animation.get(paint).apply_gradient_to_symbols(
                &[symbol],
                5,
                Some(&paint_gradient),
                None,
            );
            let smoke_scene = ch.animation.named_scene("smoke");
            ch.animation.get(smoke_scene).apply_gradient_to_symbols(
                &SMOKE_SYMBOLS,
                3,
                Some(&smoke_gradient),
                None,
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(smoke_scene),
                Action::ActivateScene(paint),
            );
            ch.animation.set_appearance(Some(symbol), base_colors);
            smoke.insert(id, smoke_scene);
        }
        while !gen_alg.complete {
            gen_alg.step(t);
        }
        let fill_alg = BreadthFirst::new(t, gen_alg.links, fill_start, limit);
        let mut active = Active::default();
        let start = fill_alg.starting_char;
        t.activate_scene(start, smoke[&start]);
        active.add(start);
        Self {
            fill_alg,
            smoke,
            active,
        }
    }
}

impl Effect for Smoke {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.fill_alg.complete && self.active.is_empty() {
            return false;
        }
        if !self.fill_alg.complete {
            self.fill_alg.step();
            for &id in &self.fill_alg.explored_last_step {
                t.activate_scene(id, self.smoke[&id]);
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
