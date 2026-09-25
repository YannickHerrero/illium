//! `bubbles`: characters are formed into bubbles that float down and pop.
use crate::engine::geometry::find_coords_on_circle;
use crate::engine::*;

const BUBBLE_COLORS: [&str; 4] = ["d33aff", "7395c4", "43c2a7", "02ff7f"];
const POP_COLOR: &str = "ffffff";
const BUBBLE_SPEED: f64 = 0.5;
const BUBBLE_DELAY: usize = 20;
const FINAL_GRADIENT_STOPS: [&str; 2] = ["d33aff", "02ff7f"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

struct Bubble {
    characters: Vec<CharId>,
    radius: i32,
    anchor: CharId,
    lowest_row: i32,
    landed: bool,
}

impl Bubble {
    fn new(t: &mut Terminal, origin: Coord, characters: Vec<CharId>) -> Self {
        let radius = (characters.len() as i32 / 5).max(1);
        let anchor = t.add_character(' ', origin);
        let lowest_row = characters
            .iter()
            .map(|&id| t.chars[id].input_coord.row)
            .min()
            .unwrap();
        let mut bubble = Self {
            characters,
            radius,
            anchor,
            lowest_row,
            landed: false,
        };
        bubble.set_character_coordinates(t);
        bubble.landed = false;
        bubble.make_waypoints(t);
        bubble.make_gradients(t);
        bubble
    }

    fn set_character_coordinates(&mut self, t: &mut Terminal) {
        let points = find_coords_on_circle(
            t.chars[self.anchor].motion.current_coord,
            self.radius,
            self.characters.len(),
            false,
        );
        for (i, &id) in self.characters.iter().enumerate() {
            let point = points[i];
            t.chars[id].motion.set_coordinate(point);
            if point.row == self.lowest_row {
                self.landed = true;
            }
        }
    }

    fn make_waypoints(&mut self, t: &mut Terminal) {
        let column = t.rng.randint(t.canvas.left as i64, t.canvas.right as i64) as i32;
        let anchor = &mut t.chars[self.anchor];
        let floor = anchor.motion.path(BUBBLE_SPEED, None);
        anchor
            .motion
            .get(floor)
            .waypoint(Coord::new(column, self.lowest_row));
        t.activate_path(self.anchor, floor);
    }

    fn make_gradients(&mut self, t: &mut Terminal) {
        let bubble_color = *t.rng.choice(&colors(&BUBBLE_COLORS));
        for &id in &self.characters {
            let ch = &mut t.chars[id];
            let sheen = ch.animation.scene();
            let symbol = ch.input_symbol;
            ch.animation
                .get(sheen)
                .add_frame(symbol, 1, ColorPair::fg(bubble_color));
            t.activate_scene(id, sheen);
        }
    }

    fn pop(&mut self, t: &mut Terminal) {
        let points = find_coords_on_circle(
            t.chars[self.anchor].motion.current_coord,
            self.radius + 3,
            self.characters.len(),
            true,
        );
        for (&id, &point) in self.characters.iter().zip(&points) {
            let ch = &mut t.chars[id];
            let pop_out = ch
                .motion
                .new_path(0.3, Some(Ease::OutExpo), None, 0, false, "pop_out");
            ch.motion.get(pop_out).waypoint(point);
            let final_path = ch.motion.query_path("final");
            ch.register(
                Event::PathComplete,
                Caller::Path(pop_out),
                Action::ActivatePath(final_path),
            );
        }
        for &id in &self.characters {
            let scene = t.chars[id].animation.query_scene("pop_1");
            t.activate_scene(id, scene);
            let path = t.chars[id].motion.query_path("pop_out");
            t.activate_path(id, path);
        }
    }

    fn activate(&self, t: &mut Terminal) {
        for &id in &self.characters {
            t.set_visible(id, true);
        }
    }

    fn move_(&mut self, t: &mut Terminal) {
        // The anchor has no scene and the characters have no path here, so a
        // tick is exactly `motion.move()` and `step_animation()` respectively.
        t.tick(self.anchor);
        self.set_character_coordinates(t);
        for &id in &self.characters {
            t.tick(id);
        }
    }
}

pub struct Bubbles {
    active: Active,
    bubbles: Vec<Bubble>,
    animating_bubbles: Vec<Bubble>,
    steps_since_last_bubble: usize,
}

impl Bubbles {
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
        let pop_color = ColorPair::fg(Color::hex(POP_COLOR));
        for id in t.input_characters() {
            let ch = &mut t.chars[id];
            let final_color = mapping[&ch.input_coord];
            ch.layer = 1;
            let pop_1 = ch.animation.named_scene("pop_1");
            let pop_2 = ch.animation.scene();
            ch.animation.get(pop_1).add_frame('*', 9, pop_color);
            ch.animation.get(pop_2).add_frame('\'', 9, pop_color);
            let final_scene = ch.animation.scene();
            let gradient = Gradient::new(&[Color::hex(POP_COLOR), final_color], &[8]);
            let symbol = ch.input_symbol;
            ch.animation.get(final_scene).apply_gradient_to_symbols(
                &[symbol],
                6,
                Some(&gradient),
                None,
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(pop_1),
                Action::ActivateScene(pop_2),
            );
            ch.register(
                Event::SceneComplete,
                Caller::Scene(pop_2),
                Action::ActivateScene(final_scene),
            );
            let final_path =
                ch.motion
                    .new_path(0.3, Some(Ease::InOutExpo), None, 0, false, "final");
            let input_coord = ch.input_coord;
            ch.motion.get(final_path).waypoint(input_coord);
            ch.register(
                Event::PathComplete,
                Caller::Path(final_path),
                Action::SetLayer(0),
            );
        }
        let mut unbubbled: Vec<CharId> = t
            .get_characters_grouped(CharacterGroup::RowBottomToTop, Select::INPUT)
            .concat();
        let mut bubbles = vec![];
        while !unbubbled.is_empty() {
            let group: Vec<CharId> = if unbubbled.len() < 5 {
                std::mem::take(&mut unbubbled)
            } else {
                let count = t.rng.randint(5, unbubbled.len().min(20) as i64) as usize;
                unbubbled.drain(..count).collect()
            };
            let origin = Coord::new(
                t.rng.randint(t.canvas.left as i64, t.canvas.right as i64) as i32,
                t.canvas.top + 10,
            );
            bubbles.push(Bubble::new(t, origin, group));
        }
        Self {
            active: Active::default(),
            bubbles,
            animating_bubbles: vec![],
            steps_since_last_bubble: 0,
        }
    }
}

impl Effect for Bubbles {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.animating_bubbles.is_empty() && self.active.is_empty() && self.bubbles.is_empty() {
            return false;
        }
        if !self.bubbles.is_empty() && self.steps_since_last_bubble >= BUBBLE_DELAY {
            let bubble = self.bubbles.remove(0);
            bubble.activate(t);
            self.animating_bubbles.push(bubble);
            self.steps_since_last_bubble = 0;
        }
        self.steps_since_last_bubble += 1;
        for bubble in &mut self.animating_bubbles {
            if bubble.landed {
                bubble.pop(t);
                for &id in &bubble.characters {
                    self.active.add(id);
                }
            }
        }
        self.animating_bubbles.retain(|b| !b.landed);
        for bubble in &mut self.animating_bubbles {
            bubble.move_(t);
        }
        self.active.update(t);
        true
    }
}
