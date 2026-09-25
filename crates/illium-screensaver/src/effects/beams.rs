//! `beams`: beams travel over the canvas illuminating the characters.
use crate::engine::*;
use std::collections::HashMap;

const BEAM_ROW_SYMBOLS: [char; 3] = ['▂', '▁', '_'];
const BEAM_COLUMN_SYMBOLS: [char; 4] = ['▌', '▍', '▎', '▏'];
const BEAM_DELAY: usize = 6;
const BEAM_ROW_SPEED_RANGE: (i64, i64) = (15, 60);
const BEAM_COLUMN_SPEED_RANGE: (i64, i64) = (9, 15);
const BEAM_GRADIENT_STOPS: [&str; 3] = ["ffffff", "00D1FF", "8A008A"];
const BEAM_GRADIENT_STEPS: [usize; 2] = [2, 6];
const BEAM_GRADIENT_FRAMES: usize = 2;
const FINAL_GRADIENT_STOPS: [&str; 3] = ["8A008A", "00D1FF", "ffffff"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 4;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Vertical;
const FINAL_WIPE_SPEED: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Beams,
    FinalWipe,
    Complete,
}

struct Group {
    characters: Vec<CharId>,
    scene: &'static str,
    speed: f64,
    next_character_counter: f64,
}

impl Group {
    fn new(mut characters: Vec<CharId>, row: bool, t: &mut Terminal) -> Self {
        let (low, high) = if row {
            BEAM_ROW_SPEED_RANGE
        } else {
            BEAM_COLUMN_SPEED_RANGE
        };
        let speed = t.rng.randint(low, high) as f64 * 0.1;
        if row {
            characters.sort_by_key(|&id| t.chars[id].input_coord.column);
        } else {
            characters.sort_by_key(|&id| t.chars[id].input_coord.row);
        }
        if *t.rng.choice(&[true, false]) {
            characters.reverse();
        }
        Self {
            characters,
            scene: if row { "beam_row" } else { "beam_column" },
            speed,
            next_character_counter: 0.0,
        }
    }

    fn get_next_character(&mut self, t: &mut Terminal) -> Option<CharId> {
        self.next_character_counter -= 1.0;
        let id = self.characters.remove(0);
        let result = if let Some(active) = t.chars[id].animation.active {
            t.chars[id].animation.get(active).reset();
            None
        } else {
            t.set_visible(id, true);
            Some(id)
        };
        let scene = t.chars[id].animation.query_scene(self.scene);
        t.activate_scene(id, scene);
        result
    }

    fn complete(&self) -> bool {
        self.characters.is_empty()
    }
}

pub struct Beams {
    pending_groups: Vec<Group>,
    active_groups: Vec<Group>,
    active: Active,
    delay: usize,
    phase: Phase,
    final_wipe_groups: Vec<Vec<CharId>>,
}

impl Beams {
    pub fn new(t: &mut Terminal) -> Self {
        let final_wipe_groups =
            t.get_characters_grouped(CharacterGroup::DiagonalTopLeftToBottomRight, Select::INPUT);
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
            .get_characters(Select::ALL_CELLS, CharacterSort::TopToBottomLeftToRight)
            .into_iter()
            .map(|id| {
                let ch = &t.chars[id];
                let color = if ch.is_fill {
                    Color::hex("000000")
                } else {
                    mapping[&ch.input_coord]
                };
                (id, color)
            })
            .collect();
        let beam_gradient = Gradient::new(&colors(&BEAM_GRADIENT_STOPS), &BEAM_GRADIENT_STEPS);
        let mut groups = vec![];
        for row in t.get_characters_grouped(CharacterGroup::RowTopToBottom, Select::ALL_CELLS) {
            groups.push(Group::new(row, true, t));
        }
        for column in t.get_characters_grouped(CharacterGroup::ColumnLeftToRight, Select::ALL_CELLS)
        {
            groups.push(Group::new(column, false, t));
        }
        for group in &groups {
            for &id in &group.characters {
                let ch = &mut t.chars[id];
                let beam_row = ch.animation.named_scene("beam_row");
                let beam_column = ch.animation.named_scene("beam_column");
                let brighten = ch.animation.named_scene("brighten");
                ch.animation.get(beam_row).apply_gradient_to_symbols(
                    &BEAM_ROW_SYMBOLS,
                    BEAM_GRADIENT_FRAMES,
                    Some(&beam_gradient),
                    None,
                );
                ch.animation.get(beam_column).apply_gradient_to_symbols(
                    &BEAM_COLUMN_SYMBOLS,
                    BEAM_GRADIENT_FRAMES,
                    Some(&beam_gradient),
                    None,
                );
                let fg = final_colors[&id];
                let faded = adjust_color_brightness(fg, 0.3);
                let fade = Gradient::new(&[fg, faded], &[10]);
                let brighten_gradient = Gradient::new(&[faded, fg], &[10]);
                let symbol = [ch.input_symbol];
                for scene in [beam_row, beam_column] {
                    ch.animation.get(scene).apply_gradient_to_symbols(
                        &symbol,
                        2,
                        Some(&fade),
                        None,
                    );
                }
                ch.animation.get(brighten).apply_gradient_to_symbols(
                    &symbol,
                    FINAL_GRADIENT_FRAMES,
                    Some(&brighten_gradient),
                    None,
                );
            }
        }
        t.rng.shuffle(&mut groups);
        Self {
            pending_groups: groups,
            active_groups: vec![],
            active: Active::default(),
            delay: 0,
            phase: Phase::Beams,
            final_wipe_groups,
        }
    }
}

impl Effect for Beams {
    fn next(&mut self, t: &mut Terminal) -> bool {
        if self.phase == Phase::Complete && self.active.is_empty() {
            return false;
        }
        match self.phase {
            Phase::Beams => {
                if self.delay == 0 {
                    if !self.pending_groups.is_empty() {
                        for _ in 0..t.rng.randint(1, 5) {
                            if !self.pending_groups.is_empty() {
                                self.active_groups.push(self.pending_groups.remove(0));
                            }
                        }
                    }
                    self.delay = BEAM_DELAY;
                } else {
                    self.delay -= 1;
                }
                for group in &mut self.active_groups {
                    group.next_character_counter += group.speed;
                    if group.next_character_counter as i64 > 1 {
                        for _ in 0..group.next_character_counter as i64 {
                            if !group.complete()
                                && let Some(id) = group.get_next_character(t)
                            {
                                self.active.add(id);
                            }
                        }
                    }
                }
                self.active_groups.retain(|g| !g.complete());
                if self.pending_groups.is_empty()
                    && self.active_groups.is_empty()
                    && self.active.is_empty()
                {
                    self.phase = Phase::FinalWipe;
                }
            }
            Phase::FinalWipe => {
                if self.final_wipe_groups.is_empty() {
                    self.phase = Phase::Complete;
                } else {
                    for _ in 0..FINAL_WIPE_SPEED {
                        if self.final_wipe_groups.is_empty() {
                            break;
                        }
                        for id in self.final_wipe_groups.remove(0) {
                            let scene = t.chars[id].animation.query_scene("brighten");
                            t.activate_scene(id, scene);
                            t.set_visible(id, true);
                            self.active.add(id);
                        }
                    }
                }
            }
            Phase::Complete => {}
        }
        self.active.update(t);
        true
    }
}
