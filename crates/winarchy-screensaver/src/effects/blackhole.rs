//! `blackhole`: characters are consumed by a black hole and explode outwards.
use crate::engine::geometry::find_coords_on_circle;
use crate::engine::*;
use std::collections::HashMap;

const BLACKHOLE_COLOR: &str = "ffffff";
const STAR_COLORS: [&str; 6] = ["ffcc0d", "ff7326", "ff194d", "bf2669", "702a8c", "049dbf"];
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 9;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

#[derive(PartialEq, Eq)]
enum Phase {
    Forming,
    Consuming,
    Collapsing,
    Exploding,
    Complete,
}

pub struct Blackhole {
    active: Active,
    blackhole_chars: Vec<CharId>,
    awaiting_consumption_chars: Vec<CharId>,
    awaiting_blackhole_chars: Vec<CharId>,
    blackhole_radius: i32,
    final_colors: HashMap<CharId, Color>,
    formation_delay: usize,
    f_delay: usize,
    phase: Phase,
}

impl Blackhole {
    pub fn new(t: &mut Terminal) -> Self {
        let c = &t.canvas;
        let blackhole_radius = round(c.width as f64 * 0.3)
            .min(round(c.height as f64 * 0.20))
            .max(3);
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let final_colors = t
            .input_characters()
            .into_iter()
            .map(|id| (id, mapping[&t.chars[id].input_coord]))
            .collect();
        let mut effect = Self {
            active: Active::default(),
            blackhole_chars: vec![],
            awaiting_consumption_chars: vec![],
            awaiting_blackhole_chars: vec![],
            blackhole_radius,
            final_colors,
            formation_delay: 0,
            f_delay: 0,
            phase: Phase::Forming,
        };
        effect.prepare_blackhole(t);
        effect.formation_delay = (100 / effect.blackhole_chars.len()).max(6);
        effect.f_delay = effect.formation_delay;
        effect.awaiting_blackhole_chars = effect.blackhole_chars.clone();
        effect
    }

    fn prepare_blackhole(&mut self, t: &mut Terminal) {
        let star_symbols = ['*', '\'', '`', '¤', '•', '°', '·'];
        let starfield_colors = Gradient::new(&colors(&["4a4a4d", "ffffff"]), &[6]).spectrum;
        let gradient_map: HashMap<Color, Gradient> = starfield_colors
            .iter()
            .map(|&color| (color, Gradient::new(&[color, Color::hex("000000")], &[10])))
            .collect();
        let mut available_chars = t.input_characters();
        while self.blackhole_chars.len() < (self.blackhole_radius * 3) as usize
            && !available_chars.is_empty()
        {
            let index = t.rng.randrange(0, available_chars.len() as i64) as usize;
            self.blackhole_chars.push(available_chars.remove(index));
        }
        let ring = find_coords_on_circle(
            t.canvas.center,
            self.blackhole_radius,
            self.blackhole_chars.len(),
            true,
        );
        for (position_index, &id) in self.blackhole_chars.iter().enumerate() {
            let ch = &mut t.chars[id];
            let path = ch
                .motion
                .new_path(0.7, Some(Ease::InOutSine), None, 0, false, "blackhole");
            ch.motion.get(path).waypoint(ring[position_index]);
            let scene = ch.animation.named_scene("blackhole");
            ch.animation
                .get(scene)
                .add_frame('*', 1, ColorPair::fg(Color::hex(BLACKHOLE_COLOR)));
            ch.register(
                Event::PathActivated,
                Caller::Path(path),
                Action::SetLayer(1),
            );
            let rotation = ch
                .motion
                .new_path(0.45, None, None, 0, true, "blackhole_rotation");
            for &coord in ring[position_index..].iter().chain(&ring[..position_index]) {
                ch.motion.get(rotation).waypoint(coord);
            }
        }
        for id in t.input_characters() {
            t.set_visible(id, true);
            let star_symbol = *t.rng.choice(&star_symbols);
            let star_color = *t.rng.choice(&starfield_colors);
            let starting = t.chars[id].animation.scene();
            t.chars[id].animation.get(starting).add_frame(
                star_symbol,
                1,
                ColorPair::fg(star_color),
            );
            t.activate_scene(id, starting);
            if !self.blackhole_chars.contains(&id) {
                let coord = t.canvas.random_coord(&mut t.rng, false, false);
                let speed = t.rng.uniform(0.17, 0.30);
                let center = t.canvas.center;
                let ch = &mut t.chars[id];
                ch.motion.set_coordinate(coord);
                let path =
                    ch.motion
                        .new_path(speed, Some(Ease::InExpo), None, 0, false, "singularity");
                ch.motion.get(path).waypoint(center);
                let consumed = ch
                    .animation
                    .new_scene(false, Some(SyncMetric::Distance), None, "");
                for &color in &gradient_map[&star_color].spectrum {
                    ch.animation
                        .get(consumed)
                        .add_frame(star_symbol, 1, ColorPair::fg(color));
                }
                ch.animation
                    .get(consumed)
                    .add_frame(' ', 1, ColorPair::default());
                ch.register(
                    Event::PathActivated,
                    Caller::Path(path),
                    Action::SetLayer(2),
                );
                ch.register(
                    Event::PathActivated,
                    Caller::Path(path),
                    Action::ActivateScene(consumed),
                );
                self.awaiting_consumption_chars.push(id);
            }
        }
        t.rng.shuffle(&mut self.awaiting_consumption_chars);
    }

    fn rotate_blackhole(&mut self, t: &mut Terminal) {
        for &id in &self.blackhole_chars {
            let path = t.chars[id].motion.query_path("blackhole_rotation");
            t.activate_path(id, path);
            self.active.add(id);
        }
    }

    fn collapse_blackhole(&mut self, t: &mut Terminal) {
        let mut ring = find_coords_on_circle(
            t.canvas.center,
            self.blackhole_radius + 3,
            self.blackhole_chars.len(),
            true,
        )
        .into_iter();
        let unstable_symbols = ['◦', '◎', '◉', '●', '◉', '◎', '◦'];
        let star_colors = colors(&STAR_COLORS);
        let center = t.canvas.center;
        let mut point_char_made = false;
        for &id in &self.blackhole_chars {
            let next_pos = ring.next().expect("pop from empty list");
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let expand = ch.motion.path(0.2, Some(Ease::InExpo));
            ch.motion.get(expand).waypoint(next_pos);
            let collapse = ch.motion.path(0.3, Some(Ease::InExpo));
            ch.motion.get(collapse).waypoint(center);
            ch.register(
                Event::PathComplete,
                Caller::Path(expand),
                Action::ActivatePath(collapse),
            );
            if !point_char_made {
                let point = ch.animation.scene();
                for _ in 0..3 {
                    for &symbol in &unstable_symbols {
                        let color = *rng.choice(&star_colors);
                        ch.animation
                            .get(point)
                            .add_frame(symbol, 3, ColorPair::fg(color));
                    }
                }
                ch.register(
                    Event::PathComplete,
                    Caller::Path(collapse),
                    Action::ActivateScene(point),
                );
                ch.register(
                    Event::PathComplete,
                    Caller::Path(collapse),
                    Action::SetLayer(3),
                );
                point_char_made = true;
            }
            t.activate_path(id, expand);
            self.active.add(id);
        }
    }

    fn explode_singularity(&mut self, t: &mut Terminal) {
        let star_colors = colors(&STAR_COLORS);
        for id in t.input_characters() {
            let input_coord = t.chars[id].input_coord;
            let nearby = find_coords_on_circle(input_coord, 3, 5, true);
            let nearby_coord = nearby[t.rng.randrange(0, 5) as usize];
            let nearby_speed = t.rng.randint(3, 4) as f64 / 10.0;
            let input_speed = t.rng.randint(4, 6) as f64 / 100.0;
            let explode_color = *t.rng.choice(&star_colors);
            let final_color = self.final_colors[&id];
            let ch = &mut t.chars[id];
            let nearby_path = ch.motion.path(nearby_speed, Some(Ease::OutExpo));
            ch.motion.get(nearby_path).waypoint(nearby_coord);
            let input_path = ch.motion.path(input_speed, Some(Ease::InCubic));
            ch.motion.get(input_path).waypoint(input_coord);
            let symbol = ch.input_symbol;
            let explode = ch.animation.scene();
            ch.animation
                .get(explode)
                .add_frame(symbol, 1, ColorPair::fg(explode_color));
            let cooling = ch.animation.scene();
            let gradient = Gradient::new(&[explode_color, final_color], &[10]);
            ch.animation.get(cooling).apply_gradient_to_symbols(
                &[symbol],
                20,
                Some(&gradient),
                None,
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(nearby_path),
                Action::ActivatePath(input_path),
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(nearby_path),
                Action::ActivateScene(cooling),
            );
            t.activate_scene(id, explode);
            t.activate_path(id, nearby_path);
            self.active.add(id);
        }
    }
}

impl Effect for Blackhole {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && self.phase == Phase::Complete {
            return false;
        }
        match self.phase {
            Phase::Forming => {
                if !self.awaiting_blackhole_chars.is_empty() {
                    if self.f_delay == 0 {
                        let id = self.awaiting_blackhole_chars.remove(0);
                        let path = t.chars[id].motion.query_path("blackhole");
                        t.activate_path(id, path);
                        let scene = t.chars[id].animation.query_scene("blackhole");
                        t.activate_scene(id, scene);
                        self.active.add(id);
                        self.f_delay = self.formation_delay;
                    } else {
                        self.f_delay -= 1;
                    }
                } else if self.active.is_empty() {
                    self.rotate_blackhole(t);
                    self.phase = Phase::Consuming;
                }
            }
            Phase::Consuming => {
                if !self.awaiting_consumption_chars.is_empty() {
                    for &id in &self.awaiting_consumption_chars {
                        let path = t.chars[id].motion.query_path("singularity");
                        t.activate_path(id, path);
                        self.active.add(id);
                    }
                    self.awaiting_consumption_chars.clear();
                } else if self
                    .active
                    .0
                    .iter()
                    .all(|id| self.blackhole_chars.contains(id))
                {
                    self.phase = Phase::Collapsing;
                }
            }
            Phase::Collapsing => {
                self.collapse_blackhole(t);
                self.phase = Phase::Exploding;
            }
            Phase::Exploding => {
                if self.blackhole_chars.iter().all(|&id| {
                    t.chars[id].motion.active.is_none() && t.chars[id].animation.active.is_none()
                }) {
                    self.explode_singularity(t);
                    self.phase = Phase::Complete;
                }
            }
            Phase::Complete => {}
        }
        self.active.update(t);
        true
    }
}
