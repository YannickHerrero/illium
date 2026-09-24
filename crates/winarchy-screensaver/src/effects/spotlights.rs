//! `spotlights`: spotlights search the text area, illuminating characters,
//! before converging in the center and expanding.
use crate::engine::geometry::{find_coords_in_circle, find_length_of_line};
use crate::engine::*;
use std::collections::{BTreeSet, HashMap};

const BEAM_WIDTH_RATIO: f64 = 2.0;
const BEAM_FALLOFF: f64 = 0.3;
const SEARCH_DURATION: usize = 550;
const SEARCH_SPEED_RANGE: (f64, f64) = (0.35, 0.75);
const SPOTLIGHT_COUNT: usize = 3;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["ab48ff", "e7b2b2", "fffebd"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Spotlights {
    active: Active,
    spotlights: Vec<CharId>,
    illuminated_chars: BTreeSet<CharId>,
    /// (bright, dark) per input character.
    character_color_map: HashMap<CharId, (ColorPair, ColorPair)>,
    illuminate_range: i32,
    search_duration: usize,
    searching: bool,
    complete: bool,
}

fn adjust_color_pair_brightness(colors: ColorPair, factor: f64) -> ColorPair {
    ColorPair::new(
        colors.fg.map(|c| adjust_color_brightness(c, factor)),
        colors.bg.map(|c| adjust_color_brightness(c, factor)),
    )
}

fn find_coord_at_minimum_distance(t: &mut Terminal, origin: Coord, minimum_distance: i32) -> Coord {
    loop {
        let coord = t.canvas.random_coord(&mut t.rng, false, false);
        if find_length_of_line(origin, coord, false) >= minimum_distance as f64 {
            return coord;
        }
    }
}

fn make_spotlights(t: &mut Terminal, count: usize) -> Vec<CharId> {
    let mut spotlights = vec![];
    let minimum_distance = t.canvas.right / 4;
    for _ in 0..count {
        let start = t.canvas.random_coord(&mut t.rng, true, false);
        let spotlight = t.add_character('O', start);
        spotlights.push(spotlight);
        let mut last_coord = t.canvas.random_coord(&mut t.rng, false, false);
        let mut targets = vec![last_coord];
        for _ in 0..10 {
            let next = find_coord_at_minimum_distance(t, last_coord, minimum_distance);
            targets.push(next);
            last_coord = next;
        }
        let mut paths = vec![];
        for coord in targets {
            let speed = t.rng.uniform(SEARCH_SPEED_RANGE.0, SEARCH_SPEED_RANGE.1);
            let control = t.canvas.random_coord(&mut t.rng, true, false);
            let motion = &mut t.chars[spotlight].motion;
            let path = motion.new_path(
                speed,
                Some(Ease::InOutQuad),
                None,
                0,
                false,
                &paths.len().to_string(),
            );
            motion.get(path).new_waypoint(coord, &[control], "");
            paths.push(path);
        }
        t.chars[spotlight].chain_paths(&paths, true);
        let center = t.canvas.center;
        let motion = &mut t.chars[spotlight].motion;
        let path = motion.new_path(0.5, Some(Ease::InOutSine), None, 0, false, "center");
        motion.get(path).waypoint(center);
    }
    spotlights
}

impl Spotlights {
    pub fn new(t: &mut Terminal) -> Self {
        let spotlights = make_spotlights(t, SPOTLIGHT_COUNT);
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let mut character_color_map = HashMap::new();
        for id in t.input_characters() {
            let bright = mapping[&t.chars[id].input_coord];
            let bright_pair = ColorPair::fg(bright);
            let dark_pair = ColorPair::fg(adjust_color_brightness(bright, 0.2));
            t.set_visible(id, true);
            character_color_map.insert(id, (bright_pair, dark_pair));
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            ch.animation.set_appearance(Some(symbol), dark_pair);
        }
        let smallest = t.canvas.right.min(t.canvas.top);
        let illuminate_range = ((smallest as f64)
            .div_euclid(BEAM_WIDTH_RATIO)
            .min(smallest as f64) as i32)
            .max(1);
        let mut active = Active::default();
        for &spotlight in &spotlights {
            let path = t.chars[spotlight].motion.query_path("0");
            t.activate_path(spotlight, path);
            active.add(spotlight);
        }
        Self {
            active,
            spotlights,
            illuminated_chars: BTreeSet::new(),
            character_color_map,
            illuminate_range,
            search_duration: SEARCH_DURATION,
            searching: true,
            complete: false,
        }
    }

    fn illuminate_chars(&mut self, t: &mut Terminal, range: i32) {
        let mut chars_in_range = BTreeSet::new();
        for &spotlight in &self.spotlights {
            for coord in find_coords_in_circle(t.chars[spotlight].motion.current_coord, range) {
                if let Some(id) = t.get_character_by_input_coord(coord)
                    && t.chars[id].input_symbol != ' '
                {
                    chars_in_range.insert(id);
                }
            }
        }
        for &id in self.illuminated_chars.difference(&chars_in_range) {
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            ch.animation
                .set_appearance(Some(symbol), self.character_color_map[&id].1);
        }
        let falloff_start = range as f64 * (1.0 - BEAM_FALLOFF);
        for &id in &chars_in_range {
            let input_coord = t.chars[id].input_coord;
            let distance = self
                .spotlights
                .iter()
                .map(|&s| find_length_of_line(t.chars[s].motion.current_coord, input_coord, true))
                .fold(f64::INFINITY, f64::min);
            let bright = self.character_color_map[&id].0;
            let adjusted = if distance > falloff_start {
                let factor =
                    (1.0 - (distance - falloff_start) / (range as f64 * BEAM_FALLOFF)).max(0.2);
                adjust_color_pair_brightness(bright, factor)
            } else {
                bright
            };
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            ch.animation.set_appearance(Some(symbol), adjusted);
        }
        self.illuminated_chars = chars_in_range;
    }
}

impl Effect for Spotlights {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.complete {
            return false;
        }
        self.illuminate_chars(t, self.illuminate_range);
        if self.searching {
            self.search_duration -= 1;
            if self.search_duration == 0 {
                for &spotlight in &self.spotlights {
                    let path = t.chars[spotlight].motion.query_path("center");
                    t.activate_path(spotlight, path);
                }
                self.searching = false;
            }
        }
        if self
            .spotlights
            .iter()
            .all(|&s| t.chars[s].motion.active.is_none())
        {
            self.spotlights.truncate(1);
            self.illuminate_range += 1;
            let limit = t.canvas.right.max(t.canvas.top) as f64;
            if self.illuminate_range as f64 > limit.div_euclid(1.5) {
                self.complete = true;
            }
        }
        self.active.update(t);
        true
    }
}
