//! `bouncyballs`: characters are bouncing balls falling from the top of the
//! canvas.
use crate::engine::*;
use std::collections::BTreeMap;

const BALL_COLORS: [&str; 3] = ["d1f4a5", "96e2a4", "5acda9"];
const BALL_SYMBOLS: [char; 5] = ['*', 'o', 'O', '0', '.'];
const BALL_DELAY: usize = 4;
const MOVEMENT_SPEED: f64 = 0.45;
const MOVEMENT_EASING: Ease = Ease::OutBounce;
const FINAL_GRADIENT_STOPS: [&str; 2] = ["f8ffae", "43c6ac"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

pub struct BouncyBalls {
    pending: Vec<CharId>,
    group_by_row: BTreeMap<i32, Vec<CharId>>,
    active: Active,
    ball_delay: usize,
}

impl BouncyBalls {
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
        let ball_colors = colors(&BALL_COLORS);
        let top = t.canvas.top;
        let mut pending = vec![];
        for id in t.input_characters() {
            let rng = &mut t.rng;
            let ch = &mut t.chars[id];
            let color = *rng.choice(&ball_colors);
            let symbol = *rng.choice(&BALL_SYMBOLS);
            let ball_scene = ch.animation.scene();
            ch.animation
                .get(ball_scene)
                .add_frame(symbol, 1, ColorPair::fg(color));
            let final_scene = ch.animation.scene();
            let input_coord = ch.input_coord;
            let gradient = Gradient::new(&[color, mapping[&input_coord]], &[10]);
            let input_symbol = ch.input_symbol;
            ch.animation.get(final_scene).apply_gradient_to_symbols(
                &[input_symbol],
                6,
                Some(&gradient),
                None,
            );
            let row = (top as f64 * rng.uniform(1.0, 1.5)) as i32;
            ch.motion
                .set_coordinate(Coord::new(input_coord.column, row));
            let path = ch.motion.path(MOVEMENT_SPEED, Some(MOVEMENT_EASING));
            ch.motion.get(path).waypoint(input_coord);
            t.activate_path(id, path);
            t.activate_scene(id, ball_scene);
            t.chars[id].register(
                Event::PathComplete,
                Caller::Path(path),
                Action::ActivateScene(final_scene),
            );
            pending.push(id);
        }
        let mut group_by_row: BTreeMap<i32, Vec<CharId>> = BTreeMap::new();
        for id in pending {
            group_by_row
                .entry(t.chars[id].input_coord.row)
                .or_default()
                .push(id);
        }
        Self {
            pending: vec![],
            group_by_row,
            active: Active::default(),
            ball_delay: 0,
        }
    }
}

impl Effect for BouncyBalls {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.group_by_row.is_empty() && self.active.is_empty() && self.pending.is_empty() {
            return false;
        }
        if self.pending.is_empty()
            && let Some((_, row)) = self.group_by_row.pop_first()
        {
            self.pending.extend(row);
        }
        if !self.pending.is_empty() {
            if self.ball_delay == 0 {
                for _ in 0..t.rng.randint(2, 6) {
                    if self.pending.is_empty() {
                        break;
                    }
                    let index = t.rng.randint(0, self.pending.len() as i64 - 1) as usize;
                    let id = self.pending.remove(index);
                    t.set_visible(id, true);
                    self.active.add(id);
                }
                self.ball_delay = BALL_DELAY;
            } else {
                self.ball_delay -= 1;
            }
        }
        self.active.update(t);
        true
    }
}
