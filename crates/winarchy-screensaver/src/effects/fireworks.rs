//! `fireworks`: characters launch and explode like fireworks and fall into place.
use crate::engine::geometry::{extrapolate_along_ray, find_coords_in_circle};
use crate::engine::*;

const EXPLODE_ANYWHERE: bool = false;
const FIREWORK_COLORS: [&str; 5] = ["88F7E2", "44D492", "F5EB67", "FFA15C", "FA233E"];
const FIREWORK_SYMBOL: char = 'o';
const FIREWORK_VOLUME: f64 = 0.05;
const LAUNCH_DELAY: i64 = 45;
const EXPLODE_DISTANCE: f64 = 0.2;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Horizontal;

pub struct Fireworks {
    active: Active,
    shells: Vec<Vec<CharId>>,
    launch_delay: i64,
}

impl Fireworks {
    pub fn new(t: &mut Terminal) -> Self {
        let firework_volume =
            round(FIREWORK_VOLUME * t.input_characters().len() as f64).max(1) as usize;
        let explode_distance = round(t.canvas.right as f64 * EXPLODE_DISTANCE).clamp(1, 15);
        let mut effect = Self {
            active: Active::default(),
            shells: vec![],
            launch_delay: 0,
        };
        effect.prepare_waypoints(t, firework_volume, explode_distance);
        effect.prepare_scenes(t);
        effect
    }

    fn prepare_waypoints(
        &mut self,
        t: &mut Terminal,
        firework_volume: usize,
        explode_distance: i32,
    ) {
        let mut shell: Vec<CharId> = vec![];
        let (mut origin_x, mut origin_coord) = (0, Coord::default());
        let mut explode_coords = vec![];
        for id in t.input_characters() {
            // The first character pushes an empty shell, launched last.
            if shell.len() == firework_volume || shell.is_empty() {
                origin_x = t.rng.randrange(0, t.canvas.right as i64) as i32;
                self.shells.push(std::mem::take(&mut shell));
                let min_row = if EXPLODE_ANYWHERE {
                    t.canvas.bottom
                } else {
                    t.chars[id].input_coord.row
                };
                let origin_y = t.rng.randrange(min_row as i64, t.canvas.top as i64 + 1) as i32;
                origin_coord = Coord::new(origin_x, origin_y);
                explode_coords = find_coords_in_circle(origin_coord, explode_distance);
            }
            let bottom = t.canvas.bottom;
            let explode_speed = t.rng.uniform(0.2, 0.4);
            let explode_coord = *t.rng.choice(&explode_coords);
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(Coord::new(origin_x, bottom));
            let apex = ch
                .motion
                .new_path(0.35, Some(Ease::OutExpo), Some(2), 0, false, "apex_pth");
            ch.motion.get(apex).waypoint(origin_coord);
            let explode =
                ch.motion
                    .new_path(explode_speed, Some(Ease::OutCirc), Some(2), 0, false, "");
            ch.motion.get(explode).waypoint(explode_coord);
            let bloom_control = extrapolate_along_ray(
                origin_coord,
                explode_coord,
                explode_distance.div_euclid(2) as f64,
            );
            let bloom_coord = Coord::new(bloom_control.column, (bloom_control.row - 7).max(1));
            ch.motion
                .get(explode)
                .new_waypoint(bloom_coord, &[bloom_control], "");
            let input =
                ch.motion
                    .new_path(0.6, Some(Ease::InOutQuart), Some(2), 0, false, "input_pth");
            let input_coord = ch.input_coord;
            ch.motion.get(input).new_waypoint(
                input_coord,
                &[Coord::new(bloom_coord.column, 1)],
                "",
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(apex),
                Action::ActivatePath(explode),
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(explode),
                Action::ActivatePath(input),
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(input),
                Action::SetLayer(0),
            );
            t.activate_path(id, apex);
            shell.push(id);
        }
        if !shell.is_empty() {
            self.shells.push(shell);
        }
    }

    fn prepare_scenes(&mut self, t: &mut Terminal) {
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let firework_colors = colors(&FIREWORK_COLORS);
        let white = Color::hex("FFFFFF");
        for shell in &self.shells {
            let shell_color = *t.rng.choice(&firework_colors);
            let shell_gradient = Gradient::new(&[shell_color, white, shell_color], &[5]);
            for &id in shell {
                let ch = &mut t.chars[id];
                let symbol = ch.input_symbol;
                let launch = ch.animation.new_scene(true, None, None, "");
                ch.animation
                    .get(launch)
                    .add_frame(FIREWORK_SYMBOL, 2, ColorPair::fg(shell_color));
                ch.animation
                    .get(launch)
                    .add_frame(FIREWORK_SYMBOL, 1, ColorPair::fg(white));
                let bloom = ch
                    .animation
                    .new_scene(false, Some(SyncMetric::Step), None, "");
                for &color in &shell_gradient.spectrum {
                    ch.animation
                        .get(bloom)
                        .add_frame(symbol, 2, ColorPair::fg(color));
                }
                let fall = ch.animation.named_scene("fall_scn");
                let fall_gradient = Gradient::new(&[shell_color, mapping[&ch.input_coord]], &[15]);
                ch.animation.get(fall).apply_gradient_to_symbols(
                    &[symbol],
                    10,
                    Some(&fall_gradient),
                    None,
                );
                t.activate_scene(id, launch);
                let ch = &mut t.chars[id];
                let apex = ch.motion.query_path("apex_pth");
                let input = ch.motion.query_path("input_pth");
                ch.register(
                    Event::PathComplete,
                    Caller::Path(apex),
                    Action::ActivateScene(bloom),
                );
                ch.register(
                    Event::PathActivated,
                    Caller::Path(input),
                    Action::ActivateScene(fall),
                );
            }
        }
    }
}

impl Effect for Fireworks {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.shells.is_empty() && self.active.is_empty() {
            return false;
        }
        if !self.shells.is_empty() && self.launch_delay <= 0 {
            for id in self.shells.pop().unwrap() {
                t.set_visible(id, true);
                self.active.add(id);
            }
            self.launch_delay = (LAUNCH_DELAY as f64 * t.rng.uniform(0.5, 1.5)) as i64;
        }
        self.launch_delay -= 1;
        self.active.update(t);
        true
    }
}
