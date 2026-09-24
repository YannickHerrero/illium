//! `orbittingvolley`: four launchers orbit the canvas firing volleys of
//! characters inward to build the input text from the center out.
use crate::engine::*;
use std::collections::HashMap;

const TOP_LAUNCHER_SYMBOL: char = '█';
const RIGHT_LAUNCHER_SYMBOL: char = '█';
const BOTTOM_LAUNCHER_SYMBOL: char = '█';
const LEFT_LAUNCHER_SYMBOL: char = '█';
const LAUNCHER_MOVEMENT_SPEED: f64 = 0.8;
const CHARACTER_MOVEMENT_SPEED: f64 = 1.5;
const VOLLEY_SIZE: f64 = 0.03;
const LAUNCH_DELAY: usize = 30;
const CHARACTER_EASING: Ease = Ease::OutSine;
const FINAL_GRADIENT_STOPS: [&str; 2] = ["FFA15C", "44D492"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Radial;

struct Launcher {
    character: CharId,
    magazine: Vec<CharId>,
}

impl Launcher {
    fn build_paths(&self, t: &mut Terminal) {
        let waypoints = [
            Coord::new(t.canvas.left, t.canvas.top),
            Coord::new(t.canvas.right, t.canvas.top),
        ];
        let ch = &mut t.chars[self.character];
        let start = waypoints
            .iter()
            .position(|&w| w == ch.input_coord)
            .expect("launcher starts on a perimeter waypoint");
        let path = ch.motion.new_path(
            LAUNCHER_MOVEMENT_SPEED,
            None,
            Some(2),
            0,
            false,
            "perimeter",
        );
        for &waypoint in waypoints[start..].iter().chain(&waypoints[..start]) {
            ch.motion.get(path).waypoint(waypoint);
        }
    }
    fn launch(&mut self, t: &mut Terminal) -> Option<CharId> {
        if self.magazine.is_empty() {
            return None;
        }
        let next = self.magazine.remove(0);
        let coord = t.chars[self.character].motion.current_coord;
        t.chars[next].motion.set_coordinate(coord);
        let path = t.chars[next].motion.query_path("input_path");
        t.activate_path(next, path);
        t.set_visible(next, true);
        Some(next)
    }
}

pub struct OrbittingVolley {
    launchers: Vec<Launcher>,
    launcher_gradient_coordinate_map: HashMap<Coord, Color>,
    delay: usize,
    complete: bool,
    active: Active,
}

impl OrbittingVolley {
    pub fn new(t: &mut Terminal) -> Self {
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let final_gradient_coordinate_map = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let launcher_gradient_coordinate_map = final_gradient.build_coordinate_color_mapping(
            c.bottom,
            c.top,
            c.left,
            c.right,
            FINAL_GRADIENT_DIRECTION,
        );
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            let input_coord = ch.input_coord;
            let path = ch.motion.new_path(
                CHARACTER_MOVEMENT_SPEED,
                Some(CHARACTER_EASING),
                Some(1),
                0,
                false,
                "input_path",
            );
            ch.motion.get(path).waypoint(input_coord);
            ch.register(Event::PathComplete, Caller::Path(path), Action::SetLayer(0));
            let symbol = ch.input_symbol;
            ch.animation.set_appearance(
                Some(symbol),
                ColorPair::fg(final_gradient_coordinate_map[&input_coord]),
            );
        }
        let c = &t.canvas;
        let corners = [
            (Coord::new(c.left, c.top), TOP_LAUNCHER_SYMBOL),
            (Coord::new(c.right, c.top), RIGHT_LAUNCHER_SYMBOL),
            (Coord::new(c.right, c.bottom), BOTTOM_LAUNCHER_SYMBOL),
            (Coord::new(c.left, c.bottom), LEFT_LAUNCHER_SYMBOL),
        ];
        let mut active = Active::default();
        let mut launchers = vec![];
        for (coord, symbol) in corners {
            let character = t.add_character(symbol, coord);
            t.chars[character].layer = 2;
            t.set_visible(character, true);
            active.add(character);
            launchers.push(Launcher {
                character,
                magazine: vec![],
            });
        }
        let main = launchers[0].character;
        let symbol = t.chars[main].input_symbol;
        t.chars[main].animation.set_appearance(
            Some(symbol),
            ColorPair::fg(*final_gradient.spectrum.last().unwrap()),
        );
        launchers[0].build_paths(t);
        let perimeter = t.chars[main].motion.query_path("perimeter");
        t.activate_path(main, perimeter);
        let sorted_chars: Vec<CharId> = t
            .get_characters_grouped(CharacterGroup::CenterToOutside, Select::INPUT)
            .into_iter()
            .flatten()
            .collect();
        let count = launchers.len();
        for (i, id) in sorted_chars.into_iter().enumerate() {
            launchers[i % count].magazine.push(id);
        }
        Self {
            launchers,
            launcher_gradient_coordinate_map,
            delay: 0,
            complete: false,
            active,
        }
    }

    fn set_launcher_coordinates(&self, t: &mut Terminal, parent: CharId, child: CharId) {
        let c = &t.canvas;
        let (top, right, bottom, left) = (c.top, c.right, c.bottom, c.left);
        let parent_progress = t.chars[parent].motion.current_coord.column as f64 / right as f64;
        let ch = &mut t.chars[child];
        if ch.input_coord == Coord::new(right, top) {
            let child_row = top - (top as f64 * parent_progress) as i32;
            ch.motion
                .set_coordinate(Coord::new(right, 1.max(child_row)));
        } else if ch.input_coord == Coord::new(right, bottom) {
            let child_column = right - (right as f64 * parent_progress) as i32;
            ch.motion
                .set_coordinate(Coord::new(1.max(child_column), bottom));
        } else if ch.input_coord == Coord::new(left, bottom) {
            let child_row = bottom + (top as f64 * parent_progress) as i32;
            ch.motion
                .set_coordinate(Coord::new(left, top.min(child_row)));
        }
        let color = self.launcher_gradient_coordinate_map[&ch.motion.current_coord];
        let symbol = ch.input_symbol;
        ch.animation
            .set_appearance(Some(symbol), ColorPair::fg(color));
    }
}

impl Effect for OrbittingVolley {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.launchers.iter().any(|l| !l.magazine.is_empty()) || self.active.len() > 1 {
            let main = self.launchers[0].character;
            if t.chars[main].motion.active.is_none() {
                let perimeter = t.chars[main].motion.query_path("perimeter");
                let start = t.chars[main].motion.paths[perimeter].waypoints[0].coord;
                t.chars[main].motion.set_coordinate(start);
                t.activate_path(main, perimeter);
                self.active.add(main);
            }
            let color = self.launcher_gradient_coordinate_map[&t.chars[main].motion.current_coord];
            t.chars[main]
                .animation
                .set_appearance(Some(TOP_LAUNCHER_SYMBOL), ColorPair::fg(color));
            for i in 1..self.launchers.len() {
                self.set_launcher_coordinates(t, main, self.launchers[i].character);
            }
            if self.delay == 0 {
                let input_count = t.input_characters().len();
                let characters_to_launch = ((VOLLEY_SIZE * input_count as f64) / 4.0) as usize;
                for launcher in &mut self.launchers {
                    for _ in 0..characters_to_launch.max(1) {
                        if let Some(next) = launcher.launch(t) {
                            self.active.add(next);
                        }
                    }
                }
                self.delay = LAUNCH_DELAY;
            } else {
                self.delay -= 1;
            }
            self.active.update(t);
            return true;
        }
        if !self.complete {
            self.complete = true;
            for launcher in &self.launchers {
                t.set_visible(launcher.character, false);
            }
            return true;
        }
        false
    }
}
