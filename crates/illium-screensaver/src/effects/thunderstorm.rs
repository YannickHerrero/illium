//! `thunderstorm`: create a thunderstorm in the terminal.
use crate::engine::*;
use crate::player::TICKS_PER_SECOND;

const LIGHTNING_COLOR: &str = "68A3E8";
const GLOWING_TEXT_COLOR: &str = "EF5411";
const RAINDROP_SYMBOLS: [char; 3] = ['\\', '.', ','];
const SPARK_SYMBOLS: [char; 3] = ['*', '.', '\''];
const SPARK_GLOW_COLOR: &str = "ff4d00";
const SPARK_GLOW_TIME: usize = 18;
const STORM_TIME: f64 = 12.0;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "FFFFFF"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

const FADE_COMPLETE: u32 = 0;
const MAKE_CHAR_GLOW: u32 = 1;
const RETURN_STRIKE_TO_POOL: u32 = 2;
const RETURN_SPARK_TO_POOL: u32 = 3;
const RETURN_RAINDROP_TO_POOL: u32 = 4;
const SET_STRIKE_IN_PROGRESS_FALSE: u32 = 5;

#[derive(PartialEq)]
enum Phase {
    PreStorm,
    Waiting,
    Storm,
    Complete,
}

pub struct Thunderstorm {
    text_chars: Vec<CharId>,
    delay: i64,
    strike_progression_delay: i64,
    rain_drops: Vec<CharId>,
    pending_strike_chars: Vec<CharId>,
    available_strike_chars: Vec<CharId>,
    active_strike_chars: Vec<CharId>,
    pending_sparks: Vec<CharId>,
    available_sparks: Vec<CharId>,
    pending_glow_chars: Vec<CharId>,
    strike_in_progress: bool,
    strike_branch_chance: f64,
    phase: Phase,
    /// `time.monotonic() - storm_start_time`, counted in frames at Omarchy's
    /// `--frame-rate 120`.
    storm_frames: usize,
    active: Active,
}

impl Thunderstorm {
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
        let mut effect = Self {
            text_chars: vec![],
            delay: 0,
            strike_progression_delay: 0,
            rain_drops: vec![],
            pending_strike_chars: vec![],
            available_strike_chars: vec![],
            active_strike_chars: vec![],
            pending_sparks: vec![],
            available_sparks: vec![],
            pending_glow_chars: vec![],
            strike_in_progress: false,
            strike_branch_chance: 0.05,
            phase: Phase::PreStorm,
            storm_frames: 0,
            active: Active::default(),
        };
        effect.build_raindrop_characters(t, 50);
        effect.build_spark_characters(t, 100);
        effect.build_strike_characters(t, 200);

        let all_chars = t.input_characters();
        for &id in &all_chars {
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            let visible = mapping[&ch.input_coord];
            let storm = adjust_color_brightness(visible, 0.5);

            let glow = ch.animation.named_scene("glow");
            let glow_gradient = Gradient::new(&[Color::hex(GLOWING_TEXT_COLOR), storm], &[7]);
            for &color in &glow_gradient.spectrum {
                ch.animation
                    .get(glow)
                    .add_frame(symbol, 6, ColorPair::fg(color));
            }

            let fade = ch.animation.named_scene("fade");
            let fade_gradient = Gradient::new(&[visible, storm], &[7]);
            for &color in &fade_gradient.spectrum {
                ch.animation
                    .get(fade)
                    .add_frame(symbol, 12, ColorPair::fg(color));
            }

            let unfade = ch.animation.named_scene("unfade");
            for &color in fade_gradient.spectrum.iter().rev() {
                ch.animation
                    .get(unfade)
                    .add_frame(symbol, 12, ColorPair::fg(color));
            }

            let lightning_flash_color = adjust_color_brightness(visible, 1.7);
            let flash = ch.animation.named_scene("flash");
            let flash_gradient = Gradient::looped(&[storm, lightning_flash_color], &[7]);
            for &color in &flash_gradient.spectrum {
                ch.animation
                    .get(flash)
                    .add_frame(symbol, 6, ColorPair::fg(color));
            }
            t.set_visible(id, true);
        }

        let reference = all_chars[0];
        let fade = t.chars[reference].animation.query_scene("fade");
        t.chars[reference].register(
            Event::SceneComplete,
            Caller::Scene(fade),
            Action::Callback(FADE_COMPLETE, 0),
        );
        effect.text_chars = all_chars;
        effect
    }

    fn make_char_glow(&mut self, t: &mut Terminal, strike_char: CharId) {
        let coord = t.chars[strike_char].motion.current_coord;
        if let Some(input) = t.get_character_by_input_coord(coord)
            && t.chars[input].visible
        {
            let glow = t.chars[input].animation.query_scene("glow");
            t.activate_scene(input, glow);
            self.pending_glow_chars.push(input);
        }
    }

    fn get_next_strike_char(&mut self, t: &mut Terminal) -> CharId {
        if self.available_strike_chars.is_empty() {
            self.build_strike_characters(t, 20);
        }
        let id = self.available_strike_chars.pop().unwrap();
        t.chars[id].animation.scenes.clear();
        t.chars[id].clear_events();
        id
    }

    fn get_next_spark_char(&mut self, t: &mut Terminal) -> CharId {
        if self.available_sparks.is_empty() {
            self.build_spark_characters(t, 20);
        }
        let id = self.available_sparks.pop().unwrap();
        t.chars[id].motion.paths.clear();
        t.chars[id].clear_events();
        id
    }

    fn setup_sparks_for_impact(&mut self, t: &mut Terminal) {
        let last = *self.pending_strike_chars.last().unwrap();
        let impact = t.chars[last].motion.current_coord;
        for _ in 0..t.rng.randint(6, 10) {
            let spark = self.get_next_spark_char(t);
            let speed = t.rng.uniform(0.1, 0.25);
            let offset = t.rng.randint(4, 20) as i32 * *t.rng.choice(&[1, -1]);
            let target = Coord::new(impact.column + offset, t.canvas.bottom);
            let bezier_column = impact.column - (impact.column - target.column).div_euclid(2);
            let bezier_row = t.rng.randint(1, t.canvas.top as i64) as i32;
            let ch = &mut t.chars[spark];
            ch.motion.set_coordinate(impact);
            let path = ch
                .motion
                .new_path(speed, Some(Ease::OutQuint), None, 30, false, "");
            ch.motion
                .get(path)
                .new_waypoint(target, &[Coord::new(bezier_column, bezier_row)], "");
            ch.register(Event::PathComplete, Caller::Path(path), Action::Hide);
            ch.register(
                Event::PathComplete,
                Caller::Path(path),
                Action::Callback(RETURN_SPARK_TO_POOL, 0),
            );
            let glow = ch.animation.query_scene("glow");
            t.activate_scene(spark, glow);
            t.activate_path(spark, path);
            self.pending_sparks.push(spark);
        }
    }

    fn setup_lightning_strike(&mut self, t: &mut Terminal, branch_neighbor: Option<CharId>) {
        let mut branch_neighbor = branch_neighbor;
        let (mut column, mut row) = match branch_neighbor {
            Some(n) => {
                let coord = t.chars[n].motion.current_coord;
                (coord.column, coord.row)
            }
            None => (t.rng.randint(1, t.canvas.right as i64) as i32, t.canvas.top),
        };
        while row >= t.canvas.bottom {
            if self.available_strike_chars.is_empty() {
                self.build_strike_characters(t, 20);
            }
            let symbol = match branch_neighbor {
                Some(n) => match t.chars[n].input_symbol {
                    '/' => {
                        column += 1;
                        *t.rng.choice(&['|', '\\'])
                    }
                    '\\' => {
                        column -= 1;
                        *t.rng.choice(&['|', '/'])
                    }
                    _ => {
                        let delta = *t.rng.choice(&[-1, 1]);
                        column += delta;
                        if delta == 1 { '\\' } else { '/' }
                    }
                },
                None => *t.rng.choice(&['\\', '/', '|']),
            };
            let strike_char = self.get_next_strike_char(t);
            let ch = &mut t.chars[strike_char];
            ch.motion.set_coordinate(Coord::new(column, row));
            ch.animation
                .set_appearance(Some(symbol), ColorPair::fg(Color::hex(LIGHTNING_COLOR)));
            row -= 1;
            if symbol == '\\' {
                column += 1;
            } else if symbol == '/' {
                column -= 1;
            }
            self.pending_strike_chars.push(strike_char);
            if t.rng.random() < self.strike_branch_chance && branch_neighbor.is_none() {
                self.strike_branch_chance -= 0.01;
                self.setup_lightning_strike(t, Some(strike_char));
            }
            branch_neighbor = None;
        }
        self.strike_branch_chance = 0.05;
        self.setup_sparks_for_impact(t);
    }

    fn build_raindrop_characters(&mut self, t: &mut Terminal, count: usize) {
        let (top, right, bottom) = (t.canvas.top, t.canvas.right, t.canvas.bottom);
        for _ in 0..count {
            let spawn_column = t.rng.randint(1 - top as i64, right as i64) as i32;
            let symbol = *t.rng.choice(&RAINDROP_SYMBOLS);
            let id = t.add_character(symbol, Coord::new(spawn_column - 1, top + 1));
            let ch = &mut t.chars[id];
            ch.layer = 1;
            ch.animation
                .set_appearance(Some(symbol), ColorPair::fg(Color::hex("aaaaff")));
            let fall = ch.motion.new_path(1.0, None, None, 0, false, "fall");
            ch.motion
                .get(fall)
                .waypoint(Coord::new(spawn_column + top, bottom - 1));
            t.activate_path(id, fall);
            t.chars[id].register(
                Event::PathComplete,
                Caller::Path(fall),
                Action::Callback(RETURN_RAINDROP_TO_POOL, 0),
            );
            t.set_visible(id, true);
            self.rain_drops.push(id);
        }
    }

    fn build_spark_characters(&mut self, t: &mut Terminal, count: usize) {
        let spark_gradient = Gradient::new(&[Color::hex(SPARK_GLOW_COLOR), t.background], &[7]);
        for _ in 0..count {
            let symbol = *t.rng.choice(&SPARK_SYMBOLS);
            let id = t.add_character(symbol, Coord::new(1, 1));
            let ch = &mut t.chars[id];
            ch.layer = 2;
            let glow = ch
                .animation
                .new_scene(false, None, Some(Ease::InCirc), "glow");
            for &color in &spark_gradient.spectrum {
                ch.animation
                    .get(glow)
                    .add_frame(symbol, SPARK_GLOW_TIME, ColorPair::fg(color));
            }
            self.available_sparks.push(id);
        }
    }

    fn build_strike_characters(&mut self, t: &mut Terminal, count: usize) {
        for _ in 0..count {
            let id = t.add_character('|', Coord::new(1, 1));
            self.available_strike_chars.push(id);
        }
    }

    fn lightning_strike(&mut self, t: &mut Terminal) {
        self.setup_lightning_strike(t, None);
        let strike_base_color = Color::hex(LIGHTNING_COLOR);
        let strike_flash_color = adjust_color_brightness(strike_base_color, 1.7);
        let strike_gradient = Gradient::looped(&[strike_base_color, strike_flash_color], &[7]);
        let fade_gradient = Gradient::new(&[strike_base_color, t.background], &[6]);
        let layer = 1;
        let flash_ease = Ease::Bezier(0.0, 1.6, 1.0, t.rng.uniform(-0.6, 0.4));
        for &id in &self.pending_strike_chars {
            let ch = &mut t.chars[id];
            let symbol = ch.animation.current.symbol;
            let flash = ch
                .animation
                .new_scene(false, None, Some(flash_ease), "flash");
            for &color in &strike_gradient.spectrum {
                ch.animation
                    .get(flash)
                    .add_frame(symbol, 6, ColorPair::fg(color));
            }
            let fade = ch.animation.named_scene("fade");
            for &color in &fade_gradient.spectrum {
                ch.animation
                    .get(fade)
                    .add_frame(symbol, 2, ColorPair::fg(color));
            }
            ch.layer = layer;
            ch.register(
                Event::SceneComplete,
                Caller::Scene(flash),
                Action::ActivateScene(fade),
            );
            ch.register(Event::SceneComplete, Caller::Scene(fade), Action::Hide);
            ch.register(
                Event::SceneComplete,
                Caller::Scene(fade),
                Action::Callback(MAKE_CHAR_GLOW, 0),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(fade),
                Action::Callback(RETURN_STRIKE_TO_POOL, 0),
            );
        }
        for &id in &self.text_chars {
            let animation = &mut t.chars[id].animation;
            let flash = animation.query_scene("flash");
            animation.get(flash).ease = Some(flash_ease);
        }
    }

    fn step_lightning_strike(&mut self, t: &mut Terminal) {
        if self.strike_progression_delay != 0 {
            self.strike_progression_delay -= 1;
            return;
        }
        if self.pending_strike_chars.is_empty() {
            return;
        }
        for _ in 0..t.rng.randint(1, 3) {
            if self.pending_strike_chars.is_empty() {
                break;
            }
            let next_strike_char = self.pending_strike_chars.remove(0);
            self.active_strike_chars.push(next_strike_char);
            t.set_visible(next_strike_char, true);
            self.strike_progression_delay = 1;
            if self.pending_strike_chars.is_empty() {
                while let Some(spark) = self.pending_sparks.pop() {
                    t.set_visible(spark, true);
                    self.active.add(spark);
                }
                let ch = &mut t.chars[next_strike_char];
                let fade = ch.animation.query_scene("fade");
                ch.register(
                    Event::SceneComplete,
                    Caller::Scene(fade),
                    Action::Callback(SET_STRIKE_IN_PROGRESS_FALSE, 0),
                );
                for strike_char in std::mem::take(&mut self.active_strike_chars) {
                    let flash = t.chars[strike_char].animation.query_scene("flash");
                    t.activate_scene(strike_char, flash);
                    self.active.add(strike_char);
                }
                for &id in &self.text_chars {
                    let flash = t.chars[id].animation.query_scene("flash");
                    t.activate_scene(id, flash);
                    self.active.add(id);
                }
            }
        }
    }

    fn rain(&mut self, t: &mut Terminal) {
        if self.rain_drops.is_empty() {
            return;
        }
        if self.delay == 0 {
            for _ in 0..t.rng.randint(1, 6) {
                if self.rain_drops.is_empty() {
                    self.build_raindrop_characters(t, 20);
                }
                let index = t.rng.randint(0, self.rain_drops.len() as i64 - 1) as usize;
                let drop = self.rain_drops.remove(index);
                let speed = t.rng.uniform(0.5, 1.5);
                let ch = &mut t.chars[drop];
                let input = ch.input_coord;
                ch.motion.set_coordinate(input);
                let fall = ch.motion.query_path("fall");
                ch.motion.get(fall).speed = speed;
                t.activate_path(drop, fall);
                self.active.add(drop);
            }
            self.delay = t.rng.randint(1, 7);
        } else {
            self.delay -= 1;
        }
    }

    fn activate_text_scene(&mut self, t: &mut Terminal, scene_id: &str) {
        for &id in &self.text_chars {
            let scene = t.chars[id].animation.query_scene(scene_id);
            t.activate_scene(id, scene);
            self.active.add(id);
        }
    }
}

impl Effect for Thunderstorm {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && self.phase == Phase::Complete {
            return false;
        }
        match self.phase {
            Phase::PreStorm => {
                self.activate_text_scene(t, "fade");
                self.phase = Phase::Waiting;
            }
            Phase::Storm => {
                self.storm_frames += 1;
                self.rain(t);
                if !self.strike_in_progress && t.rng.random() < 0.008 {
                    self.strike_in_progress = true;
                    self.lightning_strike(t);
                }
                if self.strike_in_progress {
                    self.step_lightning_strike(t);
                }
                for id in std::mem::take(&mut self.pending_glow_chars) {
                    self.active.add(id);
                }
                if self.storm_frames as f64 >= STORM_TIME * TICKS_PER_SECOND
                    && !self.strike_in_progress
                {
                    self.activate_text_scene(t, "unfade");
                    self.phase = Phase::Complete;
                }
            }
            Phase::Waiting | Phase::Complete => {}
        }
        self.active.update(t);
        for (id, tag, _) in t.take_callbacks() {
            match tag {
                FADE_COMPLETE => {
                    self.phase = Phase::Storm;
                    self.storm_frames = 0;
                }
                MAKE_CHAR_GLOW => self.make_char_glow(t, id),
                RETURN_STRIKE_TO_POOL => self.available_strike_chars.push(id),
                RETURN_SPARK_TO_POOL => self.available_sparks.push(id),
                RETURN_RAINDROP_TO_POOL => self.rain_drops.push(id),
                SET_STRIKE_IN_PROGRESS_FALSE => self.strike_in_progress = false,
                _ => unreachable!(),
            }
        }
        true
    }
}
