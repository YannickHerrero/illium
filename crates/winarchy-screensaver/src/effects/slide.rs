//! `slide`: slide characters into view from outside the terminal, grouped by
//! row (the default grouping, without merge or reverse direction).
use crate::engine::*;

const MOVEMENT_SPEED: f64 = 0.8;
const GAP: usize = 2;
const MOVEMENT_EASING: Ease = Ease::InOutQuad;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["833ab4", "fd1d1d", "fcb045"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 6;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;

pub struct Slide {
    pending_groups: Vec<Vec<CharId>>,
    active_groups: Vec<Vec<CharId>>,
    current_gap: usize,
    active: Active,
}

impl Slide {
    pub fn new(t: &mut Terminal) -> Self {
        let stops = colors(&FINAL_GRADIENT_STOPS);
        let final_gradient = Gradient::new(&stops, &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let mut groups = t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::INPUT);
        for &id in groups.iter().flatten() {
            let ch = &mut t.chars[id];
            let path = ch.motion.new_path(
                MOVEMENT_SPEED,
                Some(MOVEMENT_EASING),
                None,
                0,
                false,
                "input_path",
            );
            let input_coord = ch.input_coord;
            ch.motion.get(path).waypoint(input_coord);
        }
        let starting_column = t.canvas.left - 1;
        for group in &mut groups {
            group.reverse();
            for &id in group.iter() {
                let ch = &mut t.chars[id];
                let row = ch.input_coord.row;
                ch.motion.set_coordinate(Coord::new(starting_column, row));
                let scene = ch.animation.scene();
                let gradient = Gradient::new(&[stops[0], mapping[&ch.input_coord]], &[10]);
                let symbol = ch.input_symbol;
                ch.animation.get(scene).apply_gradient_to_symbols(
                    &[symbol],
                    FINAL_GRADIENT_FRAMES,
                    Some(&gradient),
                    None,
                );
                t.activate_scene(id, scene);
            }
        }
        Self {
            pending_groups: groups,
            active_groups: vec![],
            current_gap: 0,
            active: Active::default(),
        }
    }
}

impl Effect for Slide {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.pending_groups.is_empty() && self.active.is_empty() && self.active_groups.is_empty()
        {
            return false;
        }
        if self.current_gap == GAP && !self.pending_groups.is_empty() {
            self.active_groups.push(self.pending_groups.remove(0));
            self.current_gap = 0;
        } else if !self.pending_groups.is_empty() {
            self.current_gap += 1;
        }
        for group in &mut self.active_groups {
            if !group.is_empty() {
                let id = group.remove(0);
                t.set_visible(id, true);
                let path = t.chars[id].motion.query_path("input_path");
                t.activate_path(id, path);
                self.active.add(id);
            }
        }
        self.active_groups.retain(|g| !g.is_empty());
        self.active.update(t);
        true
    }
}
