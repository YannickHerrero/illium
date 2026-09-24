//! `unstable`: spawn characters jumbled, explode them to the edge of the
//! canvas, then reassemble them in the correct layout.
use crate::engine::*;

const UNSTABLE_COLOR: &str = "ff9200";
const EXPLOSION_EASE: Ease = Ease::OutExpo;
const EXPLOSION_SPEED: f64 = 1.0;
const REASSEMBLY_EASE: Ease = Ease::OutExpo;
const REASSEMBLY_SPEED: f64 = 1.0;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const EXPLOSION_HOLD_TIME: usize = 30;
const MAX_RUMBLE_STEPS: usize = 150;
const RUMBLE_MOD_DELAY: usize = 18;

#[derive(PartialEq)]
enum Phase {
    Rumble,
    Explosion,
    Reassembly,
}

struct Plan {
    jumbled: Coord,
    explosion: PathId,
    reassembly: PathId,
    final_scene: SceneId,
}

pub struct Unstable {
    chars: Vec<CharId>,
    plans: Vec<Plan>,
    active: Vec<usize>,
    phase: Phase,
    explosion_hold_time: usize,
    current_rumble_steps: usize,
    rumble_mod_delay: usize,
    /// Python renders the shaken frame, then restores the jumbled coords
    /// before returning; the restore waits for the next call here.
    shaken: bool,
}

impl Unstable {
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
        let unstable_color = Color::hex(UNSTABLE_COLOR);
        let chars = t.input_characters();
        let mut character_coords: Vec<Coord> =
            chars.iter().map(|&id| t.chars[id].input_coord).collect();
        let mut plans = vec![];
        for &id in &chars {
            let canvas = &t.canvas;
            let rng = &mut t.rng;
            let (column, row) = match rng.randint(0, 3) {
                0 => (canvas.left, canvas.random_row(rng, false)),
                1 => (canvas.right, canvas.random_row(rng, false)),
                2 => (canvas.random_column(rng, false), canvas.bottom),
                _ => (canvas.random_column(rng, false), canvas.top),
            };
            let index = rng.randint(0, character_coords.len() as i64 - 1) as usize;
            let jumbled = character_coords.remove(index);
            let ch = &mut t.chars[id];
            ch.motion.set_coordinate(jumbled);
            let explosion = ch.motion.new_path(
                EXPLOSION_SPEED,
                Some(EXPLOSION_EASE),
                None,
                0,
                false,
                "explosion",
            );
            ch.motion.get(explosion).waypoint(Coord::new(column, row));
            let reassembly = ch.motion.new_path(
                REASSEMBLY_SPEED,
                Some(REASSEMBLY_EASE),
                None,
                0,
                false,
                "reassembly",
            );
            let input_coord = ch.input_coord;
            ch.motion.get(reassembly).waypoint(input_coord);
            let final_color = mapping[&input_coord];
            let symbol = ch.input_symbol;
            let rumble = ch.animation.named_scene("rumble");
            let gradient = Gradient::new(&[final_color, unstable_color], &[12]);
            ch.animation.get(rumble).apply_gradient_to_symbols(
                &[symbol],
                10,
                Some(&gradient),
                None,
            );
            let final_scene = ch.animation.named_scene("final");
            let gradient = Gradient::new(&[unstable_color, final_color], &[12]);
            ch.animation.get(final_scene).apply_gradient_to_symbols(
                &[symbol],
                3,
                Some(&gradient),
                None,
            );
            t.activate_scene(id, rumble);
            t.set_visible(id, true);
            plans.push(Plan {
                jumbled,
                explosion,
                reassembly,
                final_scene,
            });
        }
        Self {
            chars,
            plans,
            active: vec![],
            phase: Phase::Rumble,
            explosion_hold_time: EXPLOSION_HOLD_TIME,
            current_rumble_steps: 0,
            rumble_mod_delay: RUMBLE_MOD_DELAY,
            shaken: false,
        }
    }

    fn at_waypoint(&self, t: &Terminal, index: usize, path: PathId) -> bool {
        let motion = &t.chars[self.chars[index]].motion;
        motion.current_coord == motion.paths[path].waypoints[0].coord
    }
}

impl Effect for Unstable {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.shaken {
            for (&id, plan) in self.chars.iter().zip(&self.plans) {
                t.chars[id].motion.set_coordinate(plan.jumbled);
            }
            self.shaken = false;
        }
        let mut frame = false;
        if self.phase == Phase::Rumble {
            if self.current_rumble_steps < MAX_RUMBLE_STEPS {
                if self.current_rumble_steps > 30
                    && self
                        .current_rumble_steps
                        .is_multiple_of(self.rumble_mod_delay)
                {
                    let row_offset = *t.rng.choice(&[-1, 0, 1]);
                    let column_offset = *t.rng.choice(&[-1, 0, 1]);
                    for &id in &self.chars {
                        let ch = &mut t.chars[id];
                        let current = ch.motion.current_coord;
                        ch.motion.set_coordinate(Coord::new(
                            current.column + column_offset,
                            current.row + row_offset,
                        ));
                        ch.animation.step(None);
                    }
                    self.shaken = true;
                    self.rumble_mod_delay = self.rumble_mod_delay.saturating_sub(1).max(1);
                } else {
                    for &id in &self.chars {
                        t.chars[id].animation.step(None);
                    }
                }
                frame = true;
                self.current_rumble_steps += 1;
            } else {
                self.phase = Phase::Explosion;
                for (&id, plan) in self.chars.iter().zip(&self.plans) {
                    t.activate_path(id, plan.explosion);
                }
                self.active = (0..self.chars.len()).collect();
            }
        }
        if self.phase == Phase::Explosion {
            if !self.active.is_empty() {
                for &i in &self.active {
                    t.tick(self.chars[i]);
                }
                let active = std::mem::take(&mut self.active);
                self.active = active
                    .into_iter()
                    .filter(|&i| !self.at_waypoint(t, i, self.plans[i].explosion))
                    .collect();
                frame = true;
            } else if self.explosion_hold_time > 0 {
                self.explosion_hold_time -= 1;
                frame = true;
            } else {
                self.phase = Phase::Reassembly;
                for (i, (&id, plan)) in self.chars.iter().zip(&self.plans).enumerate() {
                    t.activate_scene(id, plan.final_scene);
                    self.active.push(i);
                    t.activate_path(id, plan.reassembly);
                }
            }
        }
        if self.phase == Phase::Reassembly && !self.active.is_empty() {
            for &i in &self.active {
                t.tick(self.chars[i]);
            }
            let active = std::mem::take(&mut self.active);
            self.active = active
                .into_iter()
                .filter(|&i| {
                    !self.at_waypoint(t, i, self.plans[i].reassembly)
                        || !t.chars[self.chars[i]].animation.active_scene_is_complete()
                })
                .collect();
            frame = true;
        }
        frame
    }
}
