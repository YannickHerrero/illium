//! Native port of the TerminalTextEffects 0.15.0 engine, the version
//! Omarchy's screensaver runs (`tte --random-effect`).
pub mod animation;
pub mod character;
pub mod easing;
pub mod event;
pub mod geometry;
pub mod graphics;
pub mod motion;
pub mod rng;
pub mod spanningtree;
pub mod terminal;

pub use animation::{SceneId, SyncMetric, Visual};
pub use character::{CharId, Character};
pub use easing::{Ease, EasingTracker, SequenceEaser};
pub use event::{Action, Caller, Event};
pub use geometry::{Coord, round};
pub use graphics::{
    Color, ColorPair, Direction, Gradient, adjust_color_brightness, colors, shift_color_towards,
};
pub use motion::PathId;
pub use rng::Rng;
pub use terminal::{Active, Canvas, Cell, CharacterGroup, CharacterSort, Effect, Select, Terminal};
