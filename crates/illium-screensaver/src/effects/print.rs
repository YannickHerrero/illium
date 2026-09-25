//! `print`: lines are printed one at a time following a print head.
use crate::engine::*;
use std::collections::VecDeque;

const PRINT_HEAD_RETURN_SPEED: f64 = 1.5;
const PRINT_SPEED: usize = 2;
const PRINT_HEAD_EASING: Ease = Ease::InOutQuad;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["02b8bd", "c1f0e3", "00ffa0"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Diagonal;

struct Row {
    untyped_chars: VecDeque<CharId>,
    typed_chars: Vec<CharId>,
}

impl Row {
    fn new(
        t: &mut Terminal,
        characters: Vec<CharId>,
        final_color: &impl Fn(&Terminal, CharId) -> Color,
        typing_head_color: Color,
    ) -> Self {
        let characters = if characters.iter().all(|&id| t.chars[id].input_symbol == ' ') {
            characters[..1].to_vec()
        } else {
            let right_extent = characters
                .iter()
                .filter(|&&id| !t.chars[id].is_fill)
                .map(|&id| t.chars[id].input_coord.column)
                .max()
                .unwrap();
            characters
                .into_iter()
                .filter(|&id| t.chars[id].input_coord.column <= right_extent)
                .collect()
        };
        let mut row = Self {
            untyped_chars: VecDeque::new(),
            typed_chars: vec![],
        };
        for id in characters {
            let gradient = Gradient::new(&[typing_head_color, final_color(t, id)], &[5]);
            let ch = &mut t.chars[id];
            ch.motion
                .set_coordinate(Coord::new(ch.input_coord.column, 1));
            let typed = ch.animation.scene();
            let symbol = ch.input_symbol;
            ch.animation.get(typed).apply_gradient_to_symbols(
                &['█', '▓', '▒', '░', symbol],
                3,
                Some(&gradient),
                None,
            );
            t.activate_scene(id, typed);
            row.untyped_chars.push_back(id);
        }
        row
    }

    fn move_up(&self, t: &mut Terminal) {
        for &id in &self.typed_chars {
            let motion = &mut t.chars[id].motion;
            let current = motion.current_coord;
            motion.set_coordinate(Coord::new(current.column, current.row + 1));
        }
    }

    fn type_char(&mut self) -> Option<CharId> {
        let next = self.untyped_chars.pop_front()?;
        self.typed_chars.push(next);
        Some(next)
    }
}

pub struct Print {
    pending_rows: VecDeque<Row>,
    processed_rows: Vec<Row>,
    typing_head: CharId,
    current_row: Row,
    typing: bool,
    last_column: i32,
    head_event_registered: bool,
    active: Active,
}

impl Print {
    pub fn new(t: &mut Terminal) -> Self {
        let typing_head = t.add_character('█', Coord::new(1, 1));
        let final_gradient = Gradient::new(&colors(&FINAL_GRADIENT_STOPS), &[FINAL_GRADIENT_STEPS]);
        let c = &t.canvas;
        let mapping = final_gradient.build_coordinate_color_mapping(
            c.text_bottom,
            c.text_top,
            c.text_left,
            c.text_right,
            FINAL_GRADIENT_DIRECTION,
        );
        let white = Color::hex("ffffff");
        let final_color = |t: &Terminal, id: CharId| {
            mapping
                .get(&t.chars[id].input_coord)
                .copied()
                .unwrap_or(white)
        };
        let input_rows =
            t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::ALL_CELLS);
        let mut pending_rows: VecDeque<Row> = input_rows
            .into_iter()
            .map(|row| Row::new(t, row, &final_color, white))
            .collect();
        let current_row = pending_rows.pop_front().unwrap();
        Self {
            pending_rows,
            processed_rows: vec![],
            typing_head,
            current_row,
            typing: true,
            last_column: 0,
            head_event_registered: false,
            active: Active::default(),
        }
    }
}

impl Effect for Print {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.active.is_empty() && !self.typing {
            return false;
        }
        let head = self.typing_head;
        if t.chars[head].motion.active.is_some() {
        } else if !self.current_row.untyped_chars.is_empty() {
            for _ in 0..self.current_row.untyped_chars.len().min(PRINT_SPEED) {
                if let Some(id) = self.current_row.type_char() {
                    t.set_visible(id, true);
                    self.active.add(id);
                    self.last_column = t.chars[id].input_coord.column;
                }
            }
        } else if let Some(next_row) = self.pending_rows.pop_front() {
            let done = std::mem::replace(&mut self.current_row, next_row);
            self.processed_rows.push(done);
            for row in &self.processed_rows {
                row.move_up(t);
            }
            let previous = self.processed_rows.last().unwrap();
            if !previous.typed_chars.iter().all(|&id| t.chars[id].is_fill)
                && !self
                    .current_row
                    .untyped_chars
                    .iter()
                    .all(|&id| t.chars[id].is_fill)
            {
                let left_extent = self
                    .current_row
                    .untyped_chars
                    .iter()
                    .filter(|&&id| !t.chars[id].is_fill)
                    .map(|&id| t.chars[id].input_coord.column)
                    .min()
                    .unwrap();
                let text_right = t.canvas.text_right;
                self.current_row.untyped_chars.retain(|&id| {
                    (left_extent..=text_right).contains(&t.chars[id].input_coord.column)
                });
            }
            let target = t.chars[self.current_row.untyped_chars[0]]
                .input_coord
                .column;
            let ch = &mut t.chars[head];
            ch.motion.set_coordinate(Coord::new(self.last_column, 1));
            ch.visible = true;
            ch.motion.paths.clear();
            let path = ch.motion.new_path(
                PRINT_HEAD_RETURN_SPEED,
                Some(PRINT_HEAD_EASING),
                None,
                0,
                false,
                "carriage_return_path",
            );
            ch.motion.get(path).waypoint(Coord::new(target, 1));
            t.activate_path(head, path);
            // Python suppresses the duplicate registration: paths compare by id.
            if !self.head_event_registered {
                t.chars[head].register(Event::PathComplete, Caller::Path(path), Action::Hide);
                self.head_event_registered = true;
            }
            self.active.add(head);
        } else {
            self.typing = false;
        }
        self.active.update(t);
        true
    }
}
