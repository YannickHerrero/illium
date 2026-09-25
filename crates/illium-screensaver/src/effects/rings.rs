//! `rings`: characters are dispersed and form into spinning rings.
use crate::engine::geometry::{find_coords_in_rect, find_coords_on_circle};
use crate::engine::*;
use std::collections::HashMap;

const RING_COLORS: [&str; 3] = ["ab48ff", "e7b2b2", "fffebd"];
const RING_GAP: f64 = 0.1;
const SPIN_DURATION: usize = 200;
const SPIN_SPEED: (f64, f64) = (0.25, 1.0);
const DISPERSE_DURATION: usize = 200;
const SPIN_DISPERSE_CYCLES: usize = 3;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["ab48ff", "e7b2b2", "fffebd"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

struct Ring {
    counter_clockwise_coords: Vec<Coord>,
    clockwise_coords: Vec<Coord>,
    ring_gap: i32,
    ring_color: Color,
    characters: Vec<CharId>,
    character_last_ring_path: HashMap<CharId, PathId>,
    rotation_speed: f64,
}

impl Ring {
    fn new(t: &mut Terminal, ring_coords: Vec<Coord>, ring_gap: i32, ring_color: Color) -> Self {
        let mut clockwise_coords = ring_coords.clone();
        clockwise_coords.reverse();
        Self {
            counter_clockwise_coords: ring_coords,
            clockwise_coords,
            ring_gap,
            ring_color,
            characters: vec![],
            character_last_ring_path: HashMap::new(),
            rotation_speed: t.rng.uniform(SPIN_SPEED.0, SPIN_SPEED.1),
        }
    }

    fn add_character(&mut self, t: &mut Terminal, id: CharId, final_color: Color, clockwise: bool) {
        let ch = &mut t.chars[id];
        let symbol = ch.input_symbol;
        let gradient_scene = ch.animation.named_scene("gradient");
        let gradient = Gradient::new(&[final_color, self.ring_color], &[8]);
        ch.animation.get(gradient_scene).apply_gradient_to_symbols(
            &[symbol],
            3,
            Some(&gradient),
            None,
        );
        let start = self.characters.len();
        let coords = if clockwise {
            &self.clockwise_coords
        } else {
            &self.counter_clockwise_coords
        };
        let mut ring_paths = vec![];
        for &coord in coords[start..].iter().chain(&coords[..start]) {
            let path = ch.motion.new_path(
                self.rotation_speed,
                None,
                None,
                0,
                false,
                &ring_paths.len().to_string(),
            );
            ch.motion.get(path).waypoint(coord);
            ring_paths.push(path);
        }
        self.character_last_ring_path.insert(id, ring_paths[0]);
        let disperse = ch.animation.named_scene("disperse");
        let gradient = Gradient::new(&[self.ring_color, final_color], &[8]);
        ch.animation
            .get(disperse)
            .apply_gradient_to_symbols(&[symbol], 10, Some(&gradient), None);
        ch.chain_paths(&ring_paths, true);
        self.characters.push(id);
    }

    fn make_disperse_waypoints(&self, t: &mut Terminal, id: CharId, origin: Coord) -> PathId {
        let disperse_coords = find_coords_in_rect(origin, self.ring_gap);
        let ch = &mut t.chars[id];
        // `motion.paths.pop("disperse")`: the old path stays referenced by
        // earlier registrations but can no longer be found by id.
        if let Some(old) = ch.motion.paths.iter_mut().find(|p| p.id == "disperse") {
            old.id.push('\u{0}');
        }
        let path = t.chars[id]
            .motion
            .new_path(0.14, None, None, 0, true, "disperse");
        for _ in 0..5 {
            let coord = disperse_coords[t.rng.randrange(0, disperse_coords.len() as i64) as usize];
            t.chars[id].motion.get(path).waypoint(coord);
        }
        path
    }

    fn disperse(&mut self, t: &mut Terminal) {
        for &id in &self.characters {
            let motion = &t.chars[id].motion;
            let last = motion.active.unwrap_or_else(|| motion.query_path("0"));
            self.character_last_ring_path.insert(id, last);
            let current = t.chars[id].motion.current_coord;
            let path = self.make_disperse_waypoints(t, id, current);
            t.activate_path(id, path);
            let scene = t.chars[id].animation.query_scene("disperse");
            t.activate_scene(id, scene);
        }
    }

    fn spin(&mut self, t: &mut Terminal) {
        for &id in &self.characters {
            let last = self.character_last_ring_path[&id];
            let ch = &mut t.chars[id];
            let target = ch.motion.paths[last].waypoints[0].coord;
            let condense = ch.motion.path(0.1, None);
            ch.motion.get(condense).waypoint(target);
            ch.register(
                Event::PathComplete,
                Caller::Path(condense),
                Action::ActivatePath(last),
            );
            t.activate_path(id, condense);
            let scene = t.chars[id].animation.query_scene("gradient");
            t.activate_scene(id, scene);
        }
    }
}

#[derive(PartialEq, Eq)]
enum Phase {
    Start,
    Disperse,
    Spin,
    Final,
    Complete,
}

pub struct Rings {
    active: Active,
    non_ring_chars: Vec<CharId>,
    rings: Vec<Ring>,
    phase: Phase,
    initial_disperse_complete: bool,
    spin_time_remaining: usize,
    disperse_time_remaining: usize,
    cycles_remaining: usize,
    initial_phase_time_remaining: usize,
}

impl Rings {
    pub fn new(t: &mut Terminal) -> Self {
        let ring_gap = round(t.canvas.top.min(t.canvas.right) as f64 * RING_GAP).max(1);
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let mut final_colors = HashMap::new();
        let mut pending_chars = vec![];
        for id in t.input_characters() {
            let final_color = mapping[&t.chars[id].input_coord];
            final_colors.insert(id, final_color);
            let ch = &mut t.chars[id];
            let start = ch.animation.scene();
            let symbol = ch.input_symbol;
            ch.animation
                .get(start)
                .add_frame(symbol, 1, ColorPair::fg(final_color));
            let home = ch
                .motion
                .new_path(0.8, Some(Ease::OutQuad), None, 0, false, "home");
            let input_coord = ch.input_coord;
            ch.motion.get(home).waypoint(input_coord);
            t.activate_scene(id, start);
            t.set_visible(id, true);
            pending_chars.push(id);
        }
        t.rng.shuffle(&mut pending_chars);
        let ring_colors = colors(&RING_COLORS);
        let mut rings = vec![];
        let limit = t.canvas.right.max(t.canvas.top);
        for radius in (1..limit).step_by(ring_gap as usize) {
            let ring_coords =
                find_coords_on_circle(t.canvas.center, radius, 7 * radius as usize, true);
            let inside = ring_coords
                .iter()
                .filter(|&&c| t.canvas.coord_is_in_canvas(c))
                .count();
            if (inside as f64 / ring_coords.len() as f64) < 0.25 {
                break;
            }
            let color = ring_colors[rings.len() % ring_colors.len()];
            rings.push(Ring::new(t, ring_coords, ring_gap, color));
        }
        let mut ring_chars = vec![];
        for (ring_count, ring) in rings.iter_mut().enumerate() {
            for _ in 0..ring.counter_clockwise_coords.len() {
                if !pending_chars.is_empty() {
                    let id = pending_chars.remove(0);
                    ring.add_character(t, id, final_colors[&id], ring_count % 2 == 1);
                    ring_chars.push(id);
                }
            }
        }
        let mut non_ring_chars = vec![];
        for id in t.input_characters() {
            if !ring_chars.contains(&id) {
                let coord = t.canvas.random_coord(&mut t.rng, true, false);
                let ch = &mut t.chars[id];
                let external =
                    ch.motion
                        .new_path(0.8, Some(Ease::OutSine), None, 0, false, "external");
                ch.motion.get(external).waypoint(coord);
                non_ring_chars.push(id);
                ch.register(Event::PathComplete, Caller::Path(external), Action::Hide);
            }
        }
        Self {
            active: Active::default(),
            non_ring_chars,
            rings,
            phase: Phase::Start,
            initial_disperse_complete: false,
            spin_time_remaining: SPIN_DURATION,
            disperse_time_remaining: DISPERSE_DURATION,
            cycles_remaining: SPIN_DISPERSE_CYCLES,
            initial_phase_time_remaining: 100,
        }
    }
}

impl Effect for Rings {
    fn next(&mut self, t: &mut Terminal) -> bool {
        match self.phase {
            Phase::Complete => return false,
            Phase::Start => {
                if self.initial_phase_time_remaining == 0 {
                    self.phase = Phase::Disperse;
                } else {
                    self.initial_phase_time_remaining -= 1;
                }
            }
            Phase::Disperse => {
                if !self.initial_disperse_complete {
                    self.initial_disperse_complete = true;
                    for ring in &self.rings {
                        for &id in &ring.characters {
                            let first = t.chars[id].motion.query_path("0");
                            let origin = t.chars[id].motion.paths[first].waypoints[0].coord;
                            let disperse = ring.make_disperse_waypoints(t, id, origin);
                            let ch = &mut t.chars[id];
                            let target = ch.motion.paths[disperse].waypoints[0].coord;
                            let initial = ch.motion.path(0.3, Some(Ease::OutCubic));
                            ch.motion.get(initial).waypoint(target);
                            ch.register(
                                Event::PathComplete,
                                Caller::Path(initial),
                                Action::ActivatePath(disperse),
                            );
                            let scene = ch.animation.query_scene("disperse");
                            t.activate_scene(id, scene);
                            t.activate_path(id, initial);
                            self.active.add(id);
                        }
                    }
                    for &id in &self.non_ring_chars {
                        let path = t.chars[id].motion.query_path("external");
                        t.activate_path(id, path);
                        self.active.add(id);
                    }
                } else if self.disperse_time_remaining == 0 {
                    self.phase = Phase::Spin;
                    self.cycles_remaining -= 1;
                    self.spin_time_remaining = SPIN_DURATION;
                    for ring in &mut self.rings {
                        ring.spin(t);
                    }
                } else {
                    self.disperse_time_remaining -= 1;
                }
            }
            Phase::Spin => {
                if self.spin_time_remaining == 0 {
                    if self.cycles_remaining == 0 {
                        self.phase = Phase::Final;
                        for id in t.input_characters() {
                            t.set_visible(id, true);
                            let home = t.chars[id].motion.query_path("home");
                            t.activate_path(id, home);
                            self.active.add(id);
                            if t.chars[id].motion.paths.iter().any(|p| p.id == "external") {
                                continue;
                            }
                            let scene = t.chars[id].animation.query_scene("disperse");
                            t.activate_scene(id, scene);
                        }
                    } else {
                        self.disperse_time_remaining = DISPERSE_DURATION;
                        for ring in &mut self.rings {
                            ring.disperse(t);
                        }
                        self.phase = Phase::Disperse;
                    }
                } else {
                    self.spin_time_remaining -= 1;
                }
            }
            Phase::Final => {
                if self.active.is_empty() {
                    self.phase = Phase::Complete;
                }
            }
        }
        self.active.update(t);
        true
    }
}
