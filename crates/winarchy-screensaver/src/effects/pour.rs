//! `pour`: pours the characters into position from the top.
use crate::engine::*;
use std::collections::VecDeque;

const POUR_SPEED: usize = 2;
const MOVEMENT_SPEED_RANGE: (f64, f64) = (0.4, 0.6);
const GAP: usize = 1;
const STARTING_COLOR: &str = "ffffff";
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8a008a", "00d1ff", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 6;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const MOVEMENT_EASING: Ease = Ease::InQuad;

pub struct Pour {
    pending_groups: VecDeque<VecDeque<CharId>>,
    current_group: VecDeque<CharId>,
    gap: usize,
    active: Active,
}

impl Pour {
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
        let starting_color = Color::hex(STARTING_COLOR);
        let groups = t.get_characters_grouped(CharacterGroup::RowBottomToTop, Select::INPUT);
        let mut pending_groups = VecDeque::new();
        for (i, mut group) in groups.into_iter().enumerate() {
            for &id in &group {
                let speed = t
                    .rng
                    .uniform(MOVEMENT_SPEED_RANGE.0, MOVEMENT_SPEED_RANGE.1);
                let top = t.canvas.top;
                let ch = &mut t.chars[id];
                ch.visible = false;
                let input_coord = ch.input_coord;
                ch.motion
                    .set_coordinate(Coord::new(input_coord.column, top));
                let path = ch.motion.path(speed, Some(MOVEMENT_EASING));
                ch.motion.get(path).waypoint(input_coord);
                t.activate_path(id, path);
                let gradient = Gradient::new(
                    &[starting_color, mapping[&input_coord]],
                    &[FINAL_GRADIENT_STEPS],
                );
                let ch = &mut t.chars[id];
                let symbol = ch.input_symbol;
                let pour = ch.animation.scene();
                ch.animation.get(pour).apply_gradient_to_symbols(
                    &[symbol],
                    FINAL_GRADIENT_FRAMES,
                    Some(&gradient),
                    None,
                );
                t.activate_scene(id, pour);
            }
            if i % 2 == 1 {
                group.reverse();
            }
            pending_groups.push_back(VecDeque::from(group));
        }
        let current_group = pending_groups.pop_front().unwrap_or_default();
        Self {
            pending_groups,
            current_group,
            gap: 0,
            active: Active::default(),
        }
    }
}

impl Effect for Pour {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_groups.is_empty() && self.active.is_empty() && self.current_group.is_empty()
        {
            return false;
        }
        if self.current_group.is_empty()
            && let Some(group) = self.pending_groups.pop_front()
        {
            self.current_group = group;
        }
        if !self.current_group.is_empty() {
            if self.gap == 0 {
                for _ in 0..POUR_SPEED {
                    if let Some(id) = self.current_group.pop_front() {
                        t.set_visible(id, true);
                        self.active.add(id);
                    }
                }
                self.gap = GAP;
            } else {
                self.gap -= 1;
            }
        }
        self.active.update(t);
        true
    }
}
