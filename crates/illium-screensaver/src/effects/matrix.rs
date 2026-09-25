//! `matrix`: Matrix digital rain effect.
use crate::engine::*;
use crate::player::TICKS_PER_SECOND;

const HIGHLIGHT_COLOR: &str = "dbffdb";
const RAIN_COLOR_GRADIENT: [&str; 2] = ["92be92", "185318"];
const RAIN_SYMBOLS: &str = "2598Z*):.\"=+-¦|_ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ";
const RAIN_FALL_DELAY_RANGE: (i64, i64) = (2, 15);
const RAIN_COLUMN_DELAY_RANGE: (i64, i64) = (3, 9);
const RAIN_TIME: f64 = 15.0;
const SYMBOL_SWAP_CHANCE: f64 = 0.005;
const COLOR_SWAP_CHANCE: f64 = 0.001;
const RESOLVE_DELAY: usize = 3;
const FINAL_GRADIENT_STOPS: [&str; 2] = ["92be92", "336b33"];
const FINAL_GRADIENT_STEPS: usize = 12;
const FINAL_GRADIENT_FRAMES: usize = 3;
const FINAL_GRADIENT_DIRECTION: Direction = Direction::Radial;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Rain,
    Fill,
    Resolve,
}

struct RainColumn {
    characters: Vec<CharId>,
    pending_characters: Vec<CharId>,
    visible_characters: Vec<CharId>,
    column_drop_chance: f64,
    phase: Phase,
    base_rain_fall_delay: usize,
    active_rain_fall_delay: usize,
    length: usize,
    hold_time: usize,
}

struct Ctx<'a> {
    symbols: &'a [char],
    rain_colors: &'a Gradient,
    highlight: Color,
}

impl RainColumn {
    fn new(characters: Vec<CharId>, t: &mut Terminal) -> Self {
        let mut column = Self {
            characters,
            pending_characters: vec![],
            visible_characters: vec![],
            column_drop_chance: 0.08,
            phase: Phase::Rain,
            base_rain_fall_delay: 0,
            active_rain_fall_delay: 0,
            length: 0,
            hold_time: 0,
        };
        column.setup_column(Phase::Rain, t);
        column
    }

    fn setup_column(&mut self, phase: Phase, t: &mut Terminal) {
        self.pending_characters.clear();
        self.phase = phase;
        for &id in &self.characters {
            t.set_visible(id, false);
            self.pending_characters.push(id);
            let coord = t.chars[id].input_coord;
            t.chars[id].motion.current_coord = coord;
        }
        self.visible_characters = vec![];
        let (low, high) = RAIN_FALL_DELAY_RANGE;
        self.base_rain_fall_delay = if phase == Phase::Fill {
            t.rng.randint((low / 3).max(1), (high / 3).max(1))
        } else {
            t.rng.randint(low, high)
        } as usize;
        self.active_rain_fall_delay = 0;
        let len = self.characters.len();
        self.length = if phase == Phase::Rain {
            t.rng
                .randint(((len as f64 * 0.1) as i64).max(1), len as i64) as usize
        } else {
            len
        };
        self.hold_time = 0;
        if self.length == len {
            self.hold_time = t.rng.randint(20, 45) as usize;
        }
    }

    fn trim_column(&mut self, t: &mut Terminal, ctx: &Ctx) {
        if self.visible_characters.is_empty() {
            return;
        }
        let popped = self.visible_characters.remove(0);
        t.set_visible(popped, false);
        if self.visible_characters.len() > 1 {
            self.fade_last_character(t, ctx);
        }
    }

    fn drop_column(&mut self, t: &mut Terminal) {
        let bottom = t.canvas.bottom;
        self.visible_characters.retain(|&id| {
            let motion = &mut t.chars[id].motion;
            motion.current_coord.row -= 1;
            if motion.current_coord.row < bottom {
                t.chars[id].visible = false;
                return false;
            }
            true
        });
    }

    fn fade_last_character(&mut self, t: &mut Terminal, ctx: &Ctx) {
        let spectrum = &ctx.rain_colors.spectrum;
        let darker = adjust_color_brightness(*t.rng.choice(&spectrum[spectrum.len() - 3..]), 0.65);
        let animation = &mut t.chars[self.visible_characters[0]].animation;
        let symbol = animation.current.symbol;
        animation.set_appearance(Some(symbol), ColorPair::fg(darker));
    }

    fn resolve_char(&mut self, t: &mut Terminal) -> CharId {
        let index = t.rng.randint(0, self.visible_characters.len() as i64 - 1) as usize;
        self.visible_characters.remove(index)
    }

    fn tick(&mut self, t: &mut Terminal, ctx: &Ctx) {
        if self.active_rain_fall_delay == 0 {
            if !self.pending_characters.is_empty() {
                let next = self.pending_characters.remove(0);
                let symbol = *t.rng.choice(ctx.symbols);
                t.chars[next]
                    .animation
                    .set_appearance(Some(symbol), ColorPair::fg(ctx.highlight));
                if let Some(&previous) = self.visible_characters.last() {
                    let color = *t.rng.choice(&ctx.rain_colors.spectrum);
                    let animation = &mut t.chars[previous].animation;
                    let symbol = animation.current.symbol;
                    animation.set_appearance(Some(symbol), ColorPair::fg(color));
                }
                t.set_visible(next, true);
                self.visible_characters.push(next);
            } else if !self.visible_characters.is_empty() {
                let last = *self.visible_characters.last().unwrap();
                if t.chars[last].animation.current.colors.fg == Some(ctx.highlight) {
                    let color = *t.rng.choice(&ctx.rain_colors.spectrum);
                    let animation = &mut t.chars[last].animation;
                    let symbol = animation.current.symbol;
                    animation.set_appearance(Some(symbol), ColorPair::fg(color));
                }
                if self.hold_time > 0 {
                    self.hold_time -= 1;
                } else if self.phase == Phase::Rain {
                    if t.rng.random() < self.column_drop_chance {
                        self.drop_column(t);
                    }
                    self.trim_column(t, ctx);
                }
            }
            if self.visible_characters.len() > self.length {
                self.trim_column(t, ctx);
            }
            self.active_rain_fall_delay = self.base_rain_fall_delay;
        } else {
            self.active_rain_fall_delay -= 1;
        }
        for &id in &self.visible_characters {
            let current = t.chars[id].animation.current;
            let symbol = if t.rng.random() < SYMBOL_SWAP_CHANCE {
                *t.rng.choice(ctx.symbols)
            } else {
                current.symbol
            };
            let color = if t.rng.random() < COLOR_SWAP_CHANCE {
                Some(*t.rng.choice(&ctx.rain_colors.spectrum))
            } else {
                current.colors.fg
            };
            t.chars[id]
                .animation
                .set_appearance(Some(symbol), ColorPair::new(color, None));
        }
    }
}

pub struct Matrix {
    columns: Vec<RainColumn>,
    pending_columns: Vec<usize>,
    active_columns: Vec<usize>,
    full_columns: Vec<usize>,
    active: Active,
    rain_colors: Gradient,
    symbols: Vec<char>,
    column_delay: usize,
    resolve_delay: usize,
    final_frame_shown: bool,
    rain_complete: bool,
    phase: Phase,
    frame: usize,
}

impl Matrix {
    pub fn new(t: &mut Terminal) -> Self {
        let highlight = Color::hex(HIGHLIGHT_COLOR);
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
            let ch = &mut t.chars[id];
            let final_color = mapping[&ch.input_coord];
            let symbol = ch.input_symbol;
            let resolve = ch.animation.named_scene("resolve");
            for &color in &Gradient::new(&[highlight, final_color], &[8]).spectrum {
                ch.animation.get(resolve).add_frame(
                    symbol,
                    FINAL_GRADIENT_FRAMES,
                    ColorPair::fg(color),
                );
            }
        }
        let mut columns = vec![];
        for mut column in
            t.get_characters_grouped(CharacterGroup::ColumnLeftToRight, Select::ALL_CELLS)
        {
            column.reverse();
            columns.push(RainColumn::new(column, t));
        }
        let mut pending_columns: Vec<usize> = (0..columns.len()).collect();
        t.rng.shuffle(&mut pending_columns);
        Self {
            columns,
            pending_columns,
            active_columns: vec![],
            full_columns: vec![],
            active: Active::default(),
            rain_colors: Gradient::new(&colors(&RAIN_COLOR_GRADIENT), &[6]),
            symbols: RAIN_SYMBOLS.chars().collect(),
            column_delay: 0,
            resolve_delay: RESOLVE_DELAY,
            final_frame_shown: false,
            rain_complete: false,
            phase: Phase::Rain,
            frame: 0,
        }
    }
}

impl Effect for Matrix {
    fn next(&mut self, t: &mut Terminal) -> bool {
        self.frame += 1;
        let ctx = Ctx {
            symbols: &self.symbols,
            rain_colors: &self.rain_colors,
            highlight: Color::hex(HIGHLIGHT_COLOR),
        };
        match self.phase {
            Phase::Rain | Phase::Fill => {
                if self.column_delay == 0 {
                    if self.phase == Phase::Rain {
                        for _ in 0..t.rng.randint(1, 3) {
                            if !self.pending_columns.is_empty() {
                                self.active_columns.push(self.pending_columns.remove(0));
                            }
                        }
                    } else {
                        self.active_columns.append(&mut self.pending_columns);
                    }
                    self.column_delay = if self.phase == Phase::Rain {
                        let (low, high) = RAIN_COLUMN_DELAY_RANGE;
                        t.rng.randint(low, high) as usize
                    } else {
                        1
                    };
                } else {
                    self.column_delay -= 1;
                }
                for &index in &self.active_columns {
                    let column = &mut self.columns[index];
                    column.tick(t, &ctx);
                    if column.pending_characters.is_empty() {
                        if column.phase == Phase::Fill && !self.full_columns.contains(&index) {
                            self.full_columns.push(index);
                        } else if column.visible_characters.is_empty() {
                            column.setup_column(self.phase, t);
                            self.pending_columns.push(index);
                        }
                    }
                }
                let columns = &self.columns;
                self.active_columns
                    .retain(|&i| !columns[i].visible_characters.is_empty());
                if self.phase == Phase::Fill
                    && self.pending_columns.is_empty()
                    && self.active_columns.iter().all(|&i| {
                        columns[i].pending_characters.is_empty() && columns[i].phase == Phase::Fill
                    })
                {
                    self.phase = Phase::Resolve;
                    self.active_columns.clear();
                }
                // `time.time() - rain_start` at tte's 120 fps, the rate Omarchy runs.
                let elapsed = (self.frame - 1) as f64 / TICKS_PER_SECOND;
                if self.phase == Phase::Rain && RAIN_TIME > 0.0 && elapsed > RAIN_TIME {
                    self.rain_complete = true;
                    self.phase = Phase::Fill;
                    for &i in &self.active_columns {
                        self.columns[i].hold_time = 0;
                        self.columns[i].column_drop_chance = 1.0;
                    }
                    for &i in &self.pending_columns {
                        self.columns[i].setup_column(Phase::Fill, t);
                    }
                }
            }
            Phase::Resolve => {
                for &index in &self.full_columns {
                    let column = &mut self.columns[index];
                    column.tick(t, &ctx);
                    if column.visible_characters.is_empty() {
                        continue;
                    }
                    if self.resolve_delay == 0 {
                        for _ in 0..t.rng.randint(1, 4) {
                            if !column.visible_characters.is_empty() {
                                let id = column.resolve_char(t);
                                if t.chars[id].input_symbol != ' ' {
                                    let scene = t.chars[id].animation.query_scene("resolve");
                                    t.activate_scene(id, scene);
                                    self.active.add(id);
                                } else {
                                    t.set_visible(id, false);
                                }
                            }
                        }
                        self.resolve_delay = RESOLVE_DELAY;
                    } else {
                        self.resolve_delay -= 1;
                    }
                }
                let columns = &self.columns;
                self.full_columns
                    .retain(|&i| !columns[i].visible_characters.is_empty());
            }
        }
        if !self.full_columns.is_empty()
            || !self.active_columns.is_empty()
            || !self.active.is_empty()
            || !self.pending_columns.is_empty()
            || !self.rain_complete
        {
            self.active.update(t);
            return true;
        }
        if !self.final_frame_shown {
            self.final_frame_shown = true;
            self.active.update(t);
            return true;
        }
        false
    }
}
