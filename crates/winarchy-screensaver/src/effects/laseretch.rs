//! `laseretch`: a laser etches characters onto the terminal.
use crate::engine::spanningtree::RecursiveBacktracker;
use crate::engine::*;
use std::collections::{HashMap, VecDeque};

const ETCH_SPEED: usize = 1;
const ETCH_DELAY: usize = 1;
const COOL_GRADIENT_STOPS: [&str; 2] = ["ffe680", "ff7b00"];
const LASER_GRADIENT_STOPS: [&str; 2] = ["ffffff", "376cff"];
const SPARK_GRADIENT_STOPS: [&str; 4] = ["ffffff", "ffe680", "ff7b00", "1a0900"];
const SPARK_COOLING_FRAMES: usize = 7;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 8;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

struct Laser {
    position: Coord,
    beam_chars: Vec<CharId>,
    sparks: VecDeque<CharId>,
    spark_scene: HashMap<CharId, SceneId>,
}

impl Laser {
    fn new(t: &mut Terminal) -> Self {
        let mut laser_gradient: VecDeque<Color> =
            Gradient::looped(&colors(&LASER_GRADIENT_STOPS), &[6])
                .spectrum
                .into();
        let spark_gradient = Gradient::new(&colors(&SPARK_GRADIENT_STOPS), &[3, 8]);
        let mut laser = Self {
            position: Coord::new(0, 0),
            beam_chars: vec![],
            sparks: VecDeque::new(),
            spark_scene: HashMap::new(),
        };
        for _ in 0..2000 {
            let symbol = *t.rng.choice(&['.', ',', '*']);
            let id = t.add_character(symbol, laser.position);
            let ch = &mut t.chars[id];
            let spark = ch.animation.named_scene("spark");
            for &color in &spark_gradient.spectrum {
                ch.animation.get(spark).add_frame(
                    symbol,
                    SPARK_COOLING_FRAMES,
                    ColorPair::fg(color),
                );
            }
            ch.register(Event::SceneComplete, Caller::Scene(spark), Action::Hide);
            ch.layer = 2;
            laser.sparks.push_back(id);
            laser.spark_scene.insert(id, spark);
        }
        let (mut row, mut column) = (0, 0);
        while row <= t.canvas.top {
            let symbol = if laser.beam_chars.is_empty() {
                '*'
            } else {
                '/'
            };
            let id = t.add_character(symbol, Coord::new(column, row));
            t.chars[id].layer = 2;
            t.set_visible(id, true);
            row += 1;
            column += 1;
            laser.beam_chars.push(id);
            let ch = &mut t.chars[id];
            let scene = ch.animation.new_scene(true, None, None, "laser");
            for &color in &laser_gradient {
                ch.animation
                    .get(scene)
                    .add_frame(symbol, 3, ColorPair::fg(color));
            }
            laser_gradient.rotate_left(1);
            t.activate_scene(id, scene);
        }
        laser
    }

    fn reposition(&mut self, t: &mut Terminal, target: Coord, active: &mut Active) {
        self.position = target;
        let (mut row, mut column) = (target.row, target.column);
        for &id in &self.beam_chars {
            t.chars[id].motion.set_coordinate(Coord::new(column, row));
            row += 1;
            column += 1;
        }
        self.emit_sparks(t, active);
    }

    fn emit_sparks(&mut self, t: &mut Terminal, active: &mut Active) {
        let id = *self.sparks.back().unwrap();
        self.sparks.rotate_right(1);
        let position = self.position;
        let ch = &mut t.chars[id];
        ch.motion.set_coordinate(position);
        if let Some(scene) = ch.animation.active {
            ch.animation.get(scene).reset();
        }
        ch.visible = true;
        let fall_target = Coord::new(
            t.rng
                .randint(position.column as i64 - 20, position.column as i64 + 20)
                as i32,
            t.canvas.bottom,
        );
        let control = Coord::new(
            fall_target.column,
            position.row + t.rng.randint(-10, 20) as i32,
        );
        let ch = &mut t.chars[id];
        let path = ch.motion.path(0.3, Some(Ease::OutSine));
        ch.motion
            .get(path)
            .new_waypoint(fall_target, &[control], "");
        t.activate_path(id, path);
        t.activate_scene(id, self.spark_scene[&id]);
        active.add(id);
    }

    fn disable(&self, t: &mut Terminal) {
        for &id in &self.beam_chars {
            t.set_visible(id, false);
        }
    }
}

pub struct LaserEtch {
    pending_chars: VecDeque<CharId>,
    char_delay: usize,
    laser: Laser,
    active: Active,
}

impl LaserEtch {
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
        for id in t.input_characters() {
            let final_color = mapping[&t.chars[id].input_coord];
            let mut stops = colors(&COOL_GRADIENT_STOPS);
            stops.push(final_color);
            let cool_gradient = Gradient::new(&stops, &[8]);
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            let spawn = ch.animation.named_scene("spawn");
            ch.animation
                .get(spawn)
                .add_frame('^', 3, ColorPair::fg(Color::hex("ffe680")));
            for &color in &cool_gradient.spectrum {
                ch.animation
                    .get(spawn)
                    .add_frame(symbol, 3, ColorPair::fg(color));
            }
            t.activate_scene(id, spawn);
        }
        let mut algo = RecursiveBacktracker::new(t, None, true);
        while !algo.complete {
            algo.step(t);
        }
        let laser = Laser::new(t);
        let mut active = Active::default();
        for &id in &laser.beam_chars {
            active.add(id);
        }
        Self {
            pending_chars: algo.char_link_order.into(),
            char_delay: 0,
            laser,
            active,
        }
    }
}

impl Effect for LaserEtch {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_chars.is_empty() && self.active.is_empty() {
            return false;
        }
        if self.char_delay == 0 {
            for _ in 0..ETCH_SPEED {
                let Some(mut next) = self.pending_chars.pop_front() else {
                    break;
                };
                while t.chars[next].input_symbol == ' ' {
                    match self.pending_chars.pop_front() {
                        Some(id) => next = id,
                        None => break,
                    }
                }
                t.set_visible(next, true);
                self.active.add(next);
                let target = t.chars[next].input_coord;
                self.laser.reposition(t, target, &mut self.active);
            }
            self.char_delay = ETCH_DELAY;
        } else {
            self.char_delay -= 1;
        }
        if self.pending_chars.is_empty() {
            self.laser.disable(t);
        } else {
            for &id in &self.laser.beam_chars {
                self.active.add(id);
            }
        }
        self.active.update(t);
        true
    }
}
