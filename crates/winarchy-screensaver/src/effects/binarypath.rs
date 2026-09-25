//! `binarypath`: decodes characters into their binary form, which travel
//! along a path towards their input coordinates.
use crate::engine::*;

const FINAL_GRADIENT_STOPS: [&str; 2] = ["00d500", "007500"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Radial;
const BINARY_COLORS: [&str; 4] = ["044E29", "157e38", "45bf55", "95ed87"];
const MOVEMENT_SPEED: f64 = 1.0;
const ACTIVE_BINARY_GROUPS: f64 = 0.08;

struct BinaryRepresentation {
    character: CharId,
    binary_characters: Vec<CharId>,
    pending_binary_characters: Vec<CharId>,
    input_coord: Coord,
    is_active: bool,
}

impl BinaryRepresentation {
    fn travel_complete(&self, t: &Terminal) -> bool {
        self.binary_characters
            .iter()
            .all(|&id| t.chars[id].motion.current_coord == self.input_coord)
    }
    fn deactivate(&mut self, t: &mut Terminal) {
        for &id in &self.binary_characters {
            t.set_visible(id, false);
        }
        self.is_active = false;
    }
    fn activate_source_character(&self, t: &mut Terminal) {
        t.set_visible(self.character, true);
        let scene = t.chars[self.character]
            .animation
            .query_scene("collapse_scn");
        t.activate_scene(self.character, scene);
    }
}

enum Phase {
    Travel,
    Wipe,
}

pub struct BinaryPath {
    pending_binary_representations: Vec<BinaryRepresentation>,
    active_binary_reps: Vec<BinaryRepresentation>,
    final_wipe_chars: Vec<Vec<CharId>>,
    max_active_binary_groups: usize,
    last_frame_provided: bool,
    complete: bool,
    phase: Phase,
    active: Active,
}

impl BinaryPath {
    pub fn new(t: &mut Terminal) -> Self {
        let final_wipe_chars =
            t.get_characters_grouped(CharacterGroup::DiagonalTopRightToBottomLeft, Select::INPUT);
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let binary_colors = colors(&BINARY_COLORS);
        let chars = t.input_characters();
        let mut reps = vec![];
        for &id in &chars {
            let symbol = t.chars[id].animation.current.symbol;
            let binary_string = format!("{:08b}", symbol as u32);
            let mut binary_characters = vec![];
            for binary_char in binary_string.chars() {
                binary_characters.push(t.add_character(binary_char, Coord::new(0, 0)));
            }
            reps.push(BinaryRepresentation {
                character: id,
                pending_binary_characters: binary_characters.clone(),
                binary_characters,
                input_coord: t.chars[id].input_coord,
                is_active: false,
            });
        }
        let row_step_limit = 10.max((t.canvas.right as f64 * 0.2) as i64);
        for rep in &reps {
            let target = rep.input_coord;
            let starting_coord = t.canvas.random_coord(&mut t.rng, true, false);
            let mut path_coords = vec![starting_coord];
            let mut last_orientation_col = *t.rng.choice(&[true, false]);
            let mut next_coord = starting_coord;
            while *path_coords.last().unwrap() != target {
                let last = *path_coords.last().unwrap();
                let column_direction = (target.column - last.column).signum();
                let row_direction = (target.row - last.row).signum();
                let max_column_distance = (last.column - target.column).abs() as i64;
                let max_row_distance = (last.row - target.row).abs() as i64;
                if last_orientation_col && max_row_distance > 0 {
                    let step = t.rng.randint(1, max_row_distance.min(row_step_limit)) as i32;
                    next_coord = Coord::new(last.column, last.row + step * row_direction);
                    last_orientation_col = false;
                } else if !last_orientation_col && max_column_distance > 0 {
                    let step = t.rng.randint(1, max_column_distance.min(4)) as i32;
                    next_coord = Coord::new(last.column + step * column_direction, last.row);
                    last_orientation_col = true;
                } else {
                    next_coord = target;
                }
                path_coords.push(next_coord);
            }
            path_coords.push(next_coord);
            path_coords.push(target);
            for &bin in &rep.binary_characters {
                let color = *t.rng.choice(&binary_colors);
                let ch = &mut t.chars[bin];
                ch.motion.set_coordinate(path_coords[0]);
                let path = ch.motion.path(MOVEMENT_SPEED, None);
                for &coord in &path_coords {
                    ch.motion.get(path).waypoint(coord);
                }
                t.activate_path(bin, path);
                let ch = &mut t.chars[bin];
                ch.layer = 1;
                let scene = ch.animation.scene();
                let symbol = ch.animation.current.symbol;
                ch.animation
                    .get(scene)
                    .add_frame(symbol, 1, ColorPair::fg(color));
                t.activate_scene(bin, scene);
            }
        }
        let white = Color::hex("ffffff");
        for &id in &chars {
            let ch = &mut t.chars[id];
            let final_color = mapping[&ch.input_coord];
            let dim = adjust_color_brightness(final_color, 0.5);
            let symbol = ch.input_symbol;
            let collapse = ch
                .animation
                .new_scene(false, None, Some(Ease::InQuad), "collapse_scn");
            let collapse_gradient = Gradient::new(&[white, dim], &[7]);
            ch.animation.get(collapse).apply_gradient_to_symbols(
                &[symbol],
                3,
                Some(&collapse_gradient),
                None,
            );
            let brighten = ch.animation.named_scene("brighten_scn");
            let brighten_gradient = Gradient::new(&[dim, final_color], &[10]);
            ch.animation.get(brighten).apply_gradient_to_symbols(
                &[symbol],
                2,
                Some(&brighten_gradient),
                None,
            );
        }
        let max_active_binary_groups = 1.max((ACTIVE_BINARY_GROUPS * reps.len() as f64) as usize);
        Self {
            pending_binary_representations: reps,
            active_binary_reps: vec![],
            final_wipe_chars,
            max_active_binary_groups,
            last_frame_provided: false,
            complete: false,
            phase: Phase::Travel,
            active: Active::default(),
        }
    }
}

impl Effect for BinaryPath {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if !self.complete || !self.active.is_empty() {
            if let Phase::Travel = self.phase {
                while self.active_binary_reps.len() < self.max_active_binary_groups
                    && !self.pending_binary_representations.is_empty()
                {
                    let len = self.pending_binary_representations.len() as i64;
                    let index = t.rng.randrange(0, len) as usize;
                    let mut rep = self.pending_binary_representations.remove(index);
                    rep.is_active = true;
                    self.active_binary_reps.push(rep);
                }
                if !self.active_binary_reps.is_empty() {
                    for rep in &mut self.active_binary_reps {
                        if !rep.pending_binary_characters.is_empty() {
                            let next = rep.pending_binary_characters.remove(0);
                            self.active.add(next);
                            t.set_visible(next, true);
                        } else if rep.travel_complete(t) {
                            rep.deactivate(t);
                            rep.activate_source_character(t);
                            self.active.add(rep.character);
                        }
                    }
                    self.active_binary_reps.retain(|rep| rep.is_active);
                }
                if self.active.is_empty() {
                    self.phase = Phase::Wipe;
                }
            }
            if let Phase::Wipe = self.phase {
                for _ in 0..2 {
                    if self.final_wipe_chars.is_empty() {
                        self.complete = true;
                    } else {
                        for id in self.final_wipe_chars.remove(0) {
                            let scene = t.chars[id].animation.query_scene("brighten_scn");
                            t.activate_scene(id, scene);
                            t.set_visible(id, true);
                            self.active.add(id);
                        }
                    }
                }
            }
            self.active.update(t);
            return true;
        }
        if !self.last_frame_provided {
            self.last_frame_provided = true;
            return true;
        }
        false
    }
}
