//! Runs effects back to back on one monitor, like Omarchy's loop around
//! `tte --random-effect --frame-rate 120`.
use crate::effects;
use crate::engine::{Rng, Terminal};
use crate::render::Renderer;
use winarchy_config::screensaver::Effect;

pub const TICKS_PER_SECOND: f64 = 120.0;
/// `tte` exits on the final frame and the shell loop starts the next run;
/// the Python start-up keeps the finished logo on screen for about this long.
const RESTART_PAUSE_TICKS: u64 = 60;
/// Frames are never caught up by more than this after a stall.
const MAX_TICKS_PER_ADVANCE: u64 = 12;

pub struct Player {
    renderer: Renderer,
    terminal: Terminal,
    effect: Box<dyn crate::engine::Effect>,
    pub current: Effect,
    choices: Vec<Effect>,
    rng: Rng,
    ticks: f64,
    finished_for: Option<u64>,
    /// Lock mode picks a new random effect after each run; the demo repeats.
    pub shuffle: bool,
}

impl Player {
    pub fn new(choices: &[Effect], width: usize, height: usize, scale: f32, seed: u64) -> Self {
        assert!(!choices.is_empty());
        let renderer = Renderer::new(width, height, scale);
        let mut rng = Rng::new(seed);
        let first = *rng.choice(choices);
        let mut terminal = Terminal::new(
            effects::LOGO,
            renderer.columns as i32,
            renderer.rows as i32,
            rng.next_u64(),
        );
        let effect = effects::build(first, &mut terminal);
        Self {
            renderer,
            terminal,
            effect,
            current: first,
            choices: choices.to_vec(),
            rng,
            ticks: 0.0,
            finished_for: None,
            shuffle: true,
        }
    }

    /// Restarts on a given effect with a fresh canvas.
    pub fn start(&mut self, effect: Effect) {
        self.terminal = Terminal::new(
            effects::LOGO,
            self.renderer.columns as i32,
            self.renderer.rows as i32,
            self.rng.next_u64(),
        );
        self.effect = effects::build(effect, &mut self.terminal);
        self.current = effect;
        self.finished_for = None;
    }

    fn pick_next(&mut self) -> Effect {
        let others: Vec<Effect> = self
            .choices
            .iter()
            .copied()
            .filter(|&e| e != self.current)
            .collect();
        if others.is_empty() {
            self.current
        } else {
            *self.rng.choice(&others)
        }
    }

    /// Advances by `seconds` of wall time; returns whether the image changed.
    pub fn advance(&mut self, seconds: f64) -> bool {
        self.ticks += seconds * TICKS_PER_SECOND;
        let due = (self.ticks.floor() as u64).min(MAX_TICKS_PER_ADVANCE);
        self.ticks = (self.ticks - self.ticks.floor()).min(1.0);
        for _ in 0..due {
            self.step();
        }
        due > 0 && self.renderer.draw(&self.terminal.render())
    }

    fn step(&mut self) {
        match &mut self.finished_for {
            Some(n) if *n >= RESTART_PAUSE_TICKS => {
                let next = if self.shuffle {
                    self.pick_next()
                } else {
                    self.current
                };
                self.start(next);
            }
            Some(n) => *n += 1,
            None => {
                if !self.effect.next(&mut self.terminal) {
                    self.finished_for = Some(0);
                }
            }
        }
    }

    pub fn pixels(&self) -> &[u8] {
        self.renderer.pixels()
    }
    pub fn size(&self) -> (usize, usize) {
        (self.renderer.width, self.renderer.height)
    }
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }
    /// One effect frame without the wall clock (previews and tests).
    /// Returns whether that frame was one the effect produced.
    pub fn step_frame(&mut self) -> bool {
        self.step();
        self.renderer.draw(&self.terminal.render());
        self.finished_for.is_none()
    }
}
