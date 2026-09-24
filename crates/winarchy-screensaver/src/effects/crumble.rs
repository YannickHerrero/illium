//! `crumble`: characters lose color and crumble into dust, vacuumed up, and
//! reformed.
use crate::engine::*;

const FINAL_GRADIENT_STOPS: [&str; 2] = ["5CE1FF", "FF8C00"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

enum Stage {
    Falling,
    Vacuuming,
    Resetting,
    Complete,
}

pub struct Crumble {
    pending: Vec<CharId>,
    unvacuumed: Vec<CharId>,
    active: Active,
    fall_delay: i64,
    max_fall_delay: i64,
    min_fall_delay: i64,
    reset: bool,
    fall_group_maxsize: i64,
    stage: Stage,
}

impl Crumble {
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
        let (bottom, top) = (t.canvas.bottom, t.canvas.top);
        let center = Coord::new(t.canvas.center_column, t.canvas.center_row);
        let white = Color::hex("ffffff");
        let mut pending = vec![];
        for id in t.input_characters() {
            let final_color = mapping[&t.chars[id].input_coord];
            let weak_color = adjust_color_brightness(final_color, 0.65);
            let dust_color = adjust_color_brightness(final_color, 0.55);
            let strengthen_flash_gradient = Gradient::new(&[final_color, white], &[6]);
            let strengthen_gradient = Gradient::new(&[white, final_color], &[9]);
            let weaken_gradient = Gradient::new(&[weak_color, dust_color], &[9]);
            t.set_visible(id, true);
            let ch = &mut t.chars[id];
            let symbol = ch.input_symbol;
            let input_coord = ch.input_coord;
            let initial = ch.animation.scene();
            ch.animation
                .get(initial)
                .add_frame(symbol, 1, ColorPair::fg(weak_color));
            t.activate_scene(id, initial);
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let fall_path = ch.motion.path(0.65, Some(Ease::OutBounce));
            ch.motion
                .get(fall_path)
                .waypoint(Coord::new(input_coord.column, bottom));
            let weaken = ch.animation.named_scene("weaken");
            ch.animation.get(weaken).apply_gradient_to_symbols(
                &[symbol],
                4,
                Some(&weaken_gradient),
                None,
            );
            let top_path = ch
                .motion
                .new_path(1.0, Some(Ease::OutQuint), None, 0, false, "top");
            ch.motion.get(top_path).new_waypoint(
                Coord::new(input_coord.column, top),
                &[center],
                "",
            );
            let input_path = ch.motion.new_path(1.0, None, None, 0, false, "input");
            ch.motion.get(input_path).waypoint(input_coord);
            let strengthen_flash = ch.animation.scene();
            ch.animation
                .get(strengthen_flash)
                .apply_gradient_to_symbols(&[symbol], 4, Some(&strengthen_flash_gradient), None);
            let strengthen = ch.animation.scene();
            ch.animation.get(strengthen).apply_gradient_to_symbols(
                &[symbol],
                4,
                Some(&strengthen_gradient),
                None,
            );
            let dust = ch
                .animation
                .new_scene(false, Some(SyncMetric::Distance), None, "");
            for _ in 0..5 {
                let symbol = *rng.choice(&['*', '.', ',']);
                ch.animation
                    .get(dust)
                    .add_frame(symbol, 1, ColorPair::fg(dust_color));
            }
            ch.register(
                Event::SceneComplete,
                Caller::Scene(weaken),
                Action::ActivatePath(fall_path),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(weaken),
                Action::SetLayer(1),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(weaken),
                Action::ActivateScene(dust),
            );
            ch.register(
                Event::PathComplete,
                Caller::Path(input_path),
                Action::ActivateScene(strengthen_flash),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(strengthen_flash),
                Action::ActivateScene(strengthen),
            );
            pending.push(id);
        }
        t.rng.shuffle(&mut pending);
        let mut unvacuumed = t.input_characters();
        t.rng.shuffle(&mut unvacuumed);
        Self {
            pending,
            unvacuumed,
            active: Active::default(),
            fall_delay: 12,
            max_fall_delay: 12,
            min_fall_delay: 9,
            reset: false,
            fall_group_maxsize: 1,
            stage: Stage::Falling,
        }
    }
}

impl Effect for Crumble {
    fn next(&mut self, t: &mut Terminal) -> bool {
        match self.stage {
            Stage::Complete => return false,
            Stage::Falling => {
                if !self.pending.is_empty() {
                    if self.fall_delay == 0 {
                        let fall_group_size = t.rng.randint(1, self.fall_group_maxsize);
                        for _ in 0..fall_group_size {
                            if !self.pending.is_empty() {
                                let id = self.pending.remove(0);
                                let scene = t.chars[id].animation.query_scene("weaken");
                                t.activate_scene(id, scene);
                                self.active.add(id);
                            }
                        }
                        self.fall_delay = t.rng.randint(self.min_fall_delay, self.max_fall_delay);
                        if t.rng.randint(1, 10) > 4 {
                            self.fall_group_maxsize += 1;
                            self.min_fall_delay = 0.max(self.min_fall_delay - 1);
                            self.max_fall_delay = 0.max(self.max_fall_delay - 1);
                        }
                    } else {
                        self.fall_delay -= 1;
                    }
                }
                if self.pending.is_empty() && self.active.is_empty() {
                    self.stage = Stage::Vacuuming;
                }
            }
            Stage::Vacuuming => {
                if !self.unvacuumed.is_empty() {
                    for _ in 0..t.rng.randint(3, 10) {
                        if !self.unvacuumed.is_empty() {
                            let id = self.unvacuumed.remove(0);
                            let path = t.chars[id].motion.query_path("top");
                            t.activate_path(id, path);
                            self.active.add(id);
                        }
                    }
                }
                if self.active.is_empty() {
                    self.stage = Stage::Resetting;
                }
            }
            Stage::Resetting => {
                if !self.reset {
                    for id in t.input_characters() {
                        let path = t.chars[id].motion.query_path("input");
                        t.activate_path(id, path);
                        self.active.add(id);
                    }
                    self.reset = true;
                }
                if self.active.is_empty() {
                    self.stage = Stage::Complete;
                }
            }
        }
        self.active.update(t);
        true
    }
}
