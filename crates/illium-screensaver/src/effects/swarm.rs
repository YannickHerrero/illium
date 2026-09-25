//! `swarm`: characters are grouped into swarms and move around the terminal
//! before settling into position.
use crate::engine::geometry::{find_coords_in_circle, find_coords_on_circle};
use crate::engine::*;

const BASE_COLOR: [&str; 1] = ["31a0d4"];
const FLASH_COLOR: &str = "f2ea79";
const SWARM_SIZE: f64 = 0.1;
const SWARM_COORDINATION: f64 = 0.80;
const SWARM_AREA_COUNT_RANGE: (i64, i64) = (2, 4);
const FINAL_GRADIENT_STOPS: [&str; 2] = ["31b900", "f0ff65"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Horizontal;

pub struct Swarm {
    swarms: Vec<Vec<CharId>>,
    current_swarm: Vec<CharId>,
    active: Active,
    call_next: bool,
    active_swarm_area: String,
}

fn make_swarms(t: &mut Terminal, swarm_size: usize) -> Vec<Vec<CharId>> {
    let mut unswarmed = t.get_characters(Select::INPUT, CharacterSort::BottomToTopRightToLeft);
    let mut swarms = vec![];
    while !unswarmed.is_empty() {
        let mut swarm = vec![];
        for _ in 0..swarm_size {
            match unswarmed.pop() {
                Some(id) => swarm.push(id),
                None => break,
            }
        }
        swarms.push(swarm);
    }
    let final_swarm = swarms.pop().unwrap();
    if final_swarm.len() < swarm_size / 2 {
        swarms.last_mut().unwrap().extend(final_swarm);
    } else {
        swarms.push(final_swarm);
    }
    swarms
}

impl Swarm {
    pub fn new(t: &mut Terminal) -> Self {
        let character_count = t.input_characters().len();
        let swarm_size = round(character_count as f64 * SWARM_SIZE).max(1) as usize;
        let swarms = make_swarms(t, swarm_size);
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let base_colors = colors(&BASE_COLOR);
        let flash_color = Color::hex(FLASH_COLOR);
        for swarm in &swarms {
            let base = *t.rng.choice(&base_colors);
            let swarm_gradient = Gradient::new(&[base, flash_color], &[7]);
            let mut mirror = swarm_gradient.spectrum.clone();
            mirror.extend(std::iter::repeat_n(flash_color, 10));
            mirror.extend(swarm_gradient.spectrum.iter().rev());
            // A dict keyed by coord in Python: a revisited focus keeps its slot.
            let mut area_coordinate_map: Vec<(Coord, Vec<Coord>)> = vec![];
            let spawn = t.canvas.random_coord(&mut t.rng, true, false);
            let mut areas: Vec<Coord> = vec![];
            let area_count = t
                .rng
                .randint(SWARM_AREA_COUNT_RANGE.0, SWARM_AREA_COUNT_RANGE.1)
                as usize;
            let mut last_focus = spawn;
            let short_side = t.canvas.right.min(t.canvas.top);
            let radius = (short_side / 2).max(1);
            while areas.len() < area_count {
                let mut potential = find_coords_on_circle(last_focus, radius, 0, true);
                t.rng.shuffle(&mut potential);
                let next_focus = match potential
                    .into_iter()
                    .find(|&c| t.canvas.coord_is_in_canvas(c))
                {
                    Some(c) => c,
                    None => t.canvas.random_coord(&mut t.rng, false, false),
                };
                areas.push(next_focus);
                let coords = find_coords_in_circle(last_focus, (short_side / 6).max(1) * 2);
                match area_coordinate_map
                    .iter_mut()
                    .find(|(k, _)| *k == last_focus)
                {
                    Some(entry) => entry.1 = coords,
                    None => area_coordinate_map.push((last_focus, coords)),
                }
                last_focus = next_focus;
            }
            for &id in swarm {
                let ch = &mut t.chars[id];
                ch.motion.set_coordinate(spawn);
                let flash = ch
                    .animation
                    .new_scene(false, Some(SyncMetric::Distance), None, "");
                let symbol = ch.input_symbol;
                for &step in &mirror {
                    ch.animation
                        .get(flash)
                        .add_frame(symbol, 1, ColorPair::fg(step));
                }
                for (index, (_, coords)) in area_coordinate_map.iter().enumerate() {
                    let name = format!("{index}_swarm_area");
                    let rng = &mut t.rng;
                    let ch = &mut t.chars[id];
                    let origin =
                        ch.motion
                            .new_path(0.4, Some(Ease::OutSine), None, 0, false, &name);
                    let coord = *rng.choice(coords);
                    ch.motion.get(origin).new_waypoint(coord, &[], &name);
                    ch.register(
                        Event::PathActivated,
                        Caller::Path(origin),
                        Action::ActivateScene(flash),
                    );
                    ch.register(
                        Event::PathActivated,
                        Caller::Path(origin),
                        Action::SetLayer(1),
                    );
                    ch.register(
                        Event::PathComplete,
                        Caller::Path(origin),
                        Action::DeactivateScene(None),
                    );
                    for _ in 0..2 {
                        let next_coord = *rng.choice(coords);
                        let path_id = ch.motion.paths.len().to_string();
                        let inner = ch.motion.new_path(
                            0.18,
                            Some(Ease::InOutSine),
                            None,
                            0,
                            false,
                            &path_id,
                        );
                        let waypoint_id = ch.motion.paths.len().to_string();
                        ch.motion
                            .get(inner)
                            .new_waypoint(next_coord, &[], &waypoint_id);
                    }
                }
                let ch = &mut t.chars[id];
                let input_path = ch.motion.path(0.45, Some(Ease::InOutQuad));
                let input_coord = ch.input_coord;
                ch.motion.get(input_path).waypoint(input_coord);
                let input_scene = ch.animation.scene();
                let gradient = Gradient::new(&[flash_color, mapping[&input_coord]], &[10]);
                for &step in &gradient.spectrum {
                    ch.animation
                        .get(input_scene)
                        .add_frame(symbol, 3, ColorPair::fg(step));
                }
                ch.register(
                    Event::PathComplete,
                    Caller::Path(input_path),
                    Action::ActivateScene(input_scene),
                );
                ch.register(
                    Event::PathComplete,
                    Caller::Path(input_path),
                    Action::SetLayer(0),
                );
                ch.register(
                    Event::PathActivated,
                    Caller::Path(input_path),
                    Action::ActivateScene(flash),
                );
                let paths: Vec<PathId> = (0..ch.motion.paths.len()).collect();
                ch.chain_paths(&paths, false);
            }
        }
        Self {
            swarms,
            current_swarm: vec![],
            active: Active::default(),
            call_next: true,
            active_swarm_area: "0_swarm_area".to_string(),
        }
    }
}

fn area_index(id: &str) -> u32 {
    id.chars().next().and_then(|c| c.to_digit(10)).unwrap()
}

impl Effect for Swarm {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.swarms.is_empty() && self.active.is_empty() {
            return false;
        }
        if self.call_next
            && let Some(swarm) = self.swarms.pop()
        {
            self.call_next = false;
            self.current_swarm = swarm;
            self.active_swarm_area = "0_swarm_area".to_string();
            for &id in &self.current_swarm {
                let path = t.chars[id].motion.query_path("0_swarm_area");
                t.activate_path(id, path);
                t.set_visible(id, true);
                self.active.add(id);
            }
        }
        if self.active.len() < self.current_swarm.len() {
            self.call_next = true;
        }
        for (i, &id) in self.current_swarm.iter().enumerate() {
            let motion = &t.chars[id].motion;
            let Some(path) = motion.active else {
                continue;
            };
            let path_id = &motion.paths[path].id;
            if *path_id != self.active_swarm_area
                && path_id.contains("swarm_area")
                && area_index(path_id) > area_index(&self.active_swarm_area)
            {
                self.active_swarm_area = path_id.clone();
                for (j, &other) in self.current_swarm.iter().enumerate() {
                    if j != i && t.rng.random() < SWARM_COORDINATION {
                        let path = t.chars[other].motion.query_path(&self.active_swarm_area);
                        t.activate_path(other, path);
                    }
                }
                break;
            }
        }
        self.active.update(t);
        true
    }
}
