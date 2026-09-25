//! `vhstape`: lines of characters glitch left and right and lose detail like an old VHS tape.
use crate::engine::*;
use std::collections::HashMap;

const GLITCH_LINE_COLORS: [&str; 5] = ["ffffff", "ff0000", "00ff00", "0000ff", "ffffff"];
const NOISE_COLORS: [&str; 6] = ["1e1e1f", "3c3b3d", "6d6c70", "a2a1a6", "cbc9cf", "ffffff"];
const GLITCH_LINE_CHANCE: f64 = 0.05;
const NOISE_CHANCE: f64 = 0.004;
const TOTAL_GLITCH_TIME: usize = 600;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["ab48ff", "e7b2b2", "fffebd"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

const SNOW_CHARS: [char; 4] = ['#', '*', '.', ':'];

struct Line {
    characters: Vec<CharId>,
}

impl Line {
    fn new(
        t: &mut Terminal,
        characters: Vec<CharId>,
        final_colors: &HashMap<CharId, Color>,
    ) -> Self {
        let glitch_line_colors = colors(&GLITCH_LINE_COLORS);
        let noise_colors = colors(&NOISE_COLORS);
        let offset = t.rng.randint(4, 25) as i32;
        let direction = *t.rng.choice(&[-1, 1]);
        let hold_time = t.rng.randint(1, 50) as usize;
        for &id in &characters {
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let input = ch.input_coord;
            let symbol = ch.input_symbol;
            let stable = ColorPair::fg(final_colors[&id]);
            let glitch = ch
                .motion
                .new_path(2.0, None, None, hold_time, false, "glitch");
            ch.motion.get(glitch).new_waypoint(
                Coord::new(input.column + offset * direction, input.row),
                &[],
                "glitch",
            );
            let restore = ch.motion.new_path(2.0, None, None, 0, false, "restore");
            ch.motion.get(restore).new_waypoint(input, &[], "restore");
            let wave_mid = ch
                .motion
                .new_path(2.0, None, None, 0, false, "glitch_wave_mid");
            ch.motion.get(wave_mid).new_waypoint(
                Coord::new(input.column + 8, input.row),
                &[],
                "glitch_wave_mid",
            );
            let wave_end = ch
                .motion
                .new_path(2.0, None, None, 0, false, "glitch_wave_end");
            ch.motion.get(wave_end).new_waypoint(
                Coord::new(input.column + 14, input.row),
                &[],
                "glitch_wave_end",
            );

            let base = ch.animation.named_scene("base");
            ch.animation.get(base).add_frame(symbol, 1, stable);
            let forward =
                ch.animation
                    .new_scene(false, Some(SyncMetric::Step), None, "rgb_glitch_fwd");
            for &color in &glitch_line_colors {
                ch.animation
                    .get(forward)
                    .add_frame(symbol, 1, ColorPair::fg(color));
            }
            let backward =
                ch.animation
                    .new_scene(false, Some(SyncMetric::Step), None, "rgb_glitch_bwd");
            for &color in glitch_line_colors.iter().rev() {
                ch.animation
                    .get(backward)
                    .add_frame(symbol, 1, ColorPair::fg(color));
            }
            let snow = ch.animation.named_scene("snow");
            for _ in 0..25 {
                let s = *rng.choice(&SNOW_CHARS);
                let color = *rng.choice(&noise_colors);
                ch.animation.get(snow).add_frame(s, 2, ColorPair::fg(color));
            }
            ch.animation.get(snow).add_frame(symbol, 1, stable);
            let final_snow = ch.animation.named_scene("final_snow");
            let final_redraw = ch.animation.named_scene("final_redraw");
            ch.animation
                .get(final_redraw)
                .add_frame('█', 6, ColorPair::fg(Color::hex("ffffff")));
            ch.animation.get(final_redraw).add_frame(symbol, 1, stable);
            for _ in 0..30 {
                let s = *rng.choice(&SNOW_CHARS);
                let color = *rng.choice(&noise_colors);
                ch.animation
                    .get(final_snow)
                    .add_frame(s, 2, ColorPair::fg(color));
            }
            ch.register(
                Event::PathComplete,
                Caller::Path(glitch),
                Action::ActivatePath(restore),
            );
            ch.register(
                Event::PathActivated,
                Caller::Path(glitch),
                Action::ActivateScene(forward),
            );
            ch.register(
                Event::PathActivated,
                Caller::Path(restore),
                Action::ActivateScene(backward),
            );
            ch.register(
                Event::PathActivated,
                Caller::Path(wave_mid),
                Action::ActivateScene(forward),
            );
            ch.register(
                Event::PathActivated,
                Caller::Path(wave_end),
                Action::ActivateScene(forward),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(backward),
                Action::ActivateScene(base),
            );
        }
        Self { characters }
    }
    fn snow(&self, t: &mut Terminal) {
        for &id in &self.characters {
            let scene = t.chars[id].animation.query_scene("snow");
            t.activate_scene(id, scene);
        }
    }
    fn set_hold_time(&self, t: &mut Terminal, hold_time: usize) {
        for &id in &self.characters {
            let motion = &mut t.chars[id].motion;
            let path = motion.query_path("glitch");
            motion.get(path).hold_time = hold_time;
        }
    }
    fn glitch(&self, t: &mut Terminal) {
        for &id in &self.characters {
            let glitch_speed = 40.0 / t.rng.randint(20, 40) as f64;
            let restore_speed = 40.0 / t.rng.randint(20, 40) as f64;
            let motion = &mut t.chars[id].motion;
            let glitch = motion.query_path("glitch");
            let restore = motion.query_path("restore");
            motion.get(glitch).speed = glitch_speed;
            motion.get(restore).speed = restore_speed;
            t.activate_path(id, glitch);
        }
    }
    fn restore(&self, t: &mut Terminal) {
        for &id in &self.characters {
            let speed = 40.0 / t.rng.randint(20, 40) as f64;
            let motion = &mut t.chars[id].motion;
            let restore = motion.query_path("restore");
            motion.get(restore).speed = speed;
            t.activate_path(id, restore);
        }
    }
    fn activate_path(&self, t: &mut Terminal, path_id: &str) {
        for &id in &self.characters {
            let path = t.chars[id].motion.query_path(path_id);
            t.activate_path(id, path);
        }
    }
    fn line_movement_complete(&self, t: &Terminal) -> bool {
        self.characters
            .iter()
            .all(|&id| t.chars[id].motion.movement_is_complete())
    }
}

#[derive(PartialEq)]
enum Phase {
    Glitching,
    Noise,
    Redraw,
    Complete,
}

pub struct VhsTape {
    lines: Vec<Line>,
    active_glitch_wave_top: Option<i32>,
    active_glitch_wave_lines: Vec<usize>,
    active_glitch_lines: Vec<usize>,
    glitching_steps_elapsed: usize,
    phase: Phase,
    to_redraw: Vec<usize>,
    redrawing: bool,
    active: Active,
}

impl VhsTape {
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
        let final_colors: HashMap<CharId, Color> = t
            .input_characters()
            .into_iter()
            .map(|id| (id, mapping[&t.chars[id].input_coord]))
            .collect();
        let rows = t.get_characters_grouped(CharacterGroup::RowBottomToTop, Select::INPUT);
        let lines: Vec<Line> = rows
            .into_iter()
            .map(|row| Line::new(t, row, &final_colors))
            .collect();
        for id in t.input_characters() {
            t.set_visible(id, true);
            let scene = t.chars[id].animation.query_scene("base");
            t.activate_scene(id, scene);
        }
        let to_redraw = (0..lines.len()).collect();
        Self {
            lines,
            active_glitch_wave_top: None,
            active_glitch_wave_lines: vec![],
            active_glitch_lines: vec![],
            glitching_steps_elapsed: 0,
            phase: Phase::Glitching,
            to_redraw,
            redrawing: false,
            active: Active::default(),
        }
    }

    fn activate_line(&mut self, line: usize) {
        for &id in &self.lines[line].characters {
            self.active.add(id);
        }
    }

    fn glitch_wave(&mut self, t: &mut Terminal) {
        let c = t.canvas.clone();
        if self.active_glitch_wave_top.is_none() {
            if c.text_height >= 3 {
                let low = 3.max(round(c.text_height as f64 * 0.5));
                self.active_glitch_wave_top =
                    Some(c.text_bottom + t.rng.randint(low as i64, c.text_height as i64) as i32);
            } else {
                return;
            }
        }
        if self
            .active_glitch_wave_lines
            .iter()
            .all(|&l| self.lines[l].line_movement_complete(t))
        {
            let mut top = self.active_glitch_wave_top.unwrap();
            if !self.active_glitch_wave_lines.is_empty() {
                let wave_top_delta = if t.rng.random() < 0.3 {
                    if t.rng.random() < 0.3 { 1 } else { -1 }
                } else {
                    0
                };
                top += wave_top_delta;
                top = 2.max(top.min(c.text_top));
            }
            self.active_glitch_wave_top = Some(top);
            let mut new_wave_lines = vec![];
            for line_index in top - 2..top + 1 {
                let adjusted = line_index - (c.text_bottom - 1);
                if adjusted >= 0 && (adjusted as usize) < self.lines.len() {
                    new_wave_lines.push(adjusted as usize);
                }
            }
            for line in std::mem::take(&mut self.active_glitch_wave_lines) {
                if !new_wave_lines.contains(&line) {
                    self.lines[line].restore(t);
                    self.activate_line(line);
                }
            }
            self.active_glitch_wave_lines = new_wave_lines;
            if top < c.text_bottom + 2 {
                for line in std::mem::take(&mut self.active_glitch_wave_lines) {
                    self.lines[line].restore(t);
                    self.activate_line(line);
                }
                self.active_glitch_wave_top = None;
            } else {
                let lines = self.active_glitch_wave_lines.clone();
                for (line, path_id) in
                    lines
                        .into_iter()
                        .zip(["glitch_wave_mid", "glitch_wave_end", "glitch_wave_mid"])
                {
                    self.lines[line].activate_path(t, path_id);
                    self.activate_line(line);
                }
            }
        }
    }
}

impl Effect for VhsTape {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.phase == Phase::Complete && self.active.is_empty() {
            return false;
        }
        match self.phase {
            Phase::Glitching => {
                if self.active_glitch_wave_lines.is_empty()
                    || self
                        .active_glitch_wave_lines
                        .iter()
                        .all(|&l| self.lines[l].line_movement_complete(t))
                {
                    self.glitch_wave(t);
                }
                let lines = &self.lines;
                self.active_glitch_lines
                    .retain(|&l| !lines[l].line_movement_complete(t));
                if t.rng.random() < GLITCH_LINE_CHANCE && self.active_glitch_lines.len() < 3 {
                    let line = t.rng.randrange(0, self.lines.len() as i64) as usize;
                    if !self.active_glitch_wave_lines.contains(&line)
                        && !self.active_glitch_lines.contains(&line)
                    {
                        let hold_time = t.rng.randint(20, 75) as usize;
                        self.lines[line].set_hold_time(t, hold_time);
                        self.active_glitch_lines.push(line);
                        self.lines[line].glitch(t);
                        self.activate_line(line);
                    }
                }
                if t.rng.random() < NOISE_CHANCE {
                    for line in 0..self.lines.len() {
                        self.lines[line].snow(t);
                        if !self.active_glitch_wave_lines.contains(&line)
                            && !self.active_glitch_lines.contains(&line)
                        {
                            self.activate_line(line);
                        }
                    }
                }
                self.glitching_steps_elapsed += 1;
                if self.glitching_steps_elapsed >= TOTAL_GLITCH_TIME {
                    for &line in &self.active_glitch_wave_lines {
                        self.lines[line].restore(t);
                    }
                    for &line in &self.active_glitch_lines {
                        self.lines[line].restore(t);
                    }
                    self.phase = Phase::Noise;
                }
            }
            Phase::Noise => {
                if self.active.is_empty() {
                    for id in t.input_characters() {
                        let scene = t.chars[id].animation.query_scene("final_snow");
                        t.activate_scene(id, scene);
                        self.active.add(id);
                    }
                    self.phase = Phase::Redraw;
                }
            }
            Phase::Redraw => {
                if self.redrawing || self.active.is_empty() {
                    self.redrawing = true;
                    if let Some(line) = self.to_redraw.pop() {
                        for &id in &self.lines[line].characters {
                            let scene = t.chars[id].animation.query_scene("final_redraw");
                            t.activate_scene(id, scene);
                            self.active.add(id);
                        }
                    } else {
                        self.phase = Phase::Complete;
                    }
                }
            }
            Phase::Complete => {}
        }
        self.active.update(t);
        true
    }
}
