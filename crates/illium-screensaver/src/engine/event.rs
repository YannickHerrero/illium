//! `EventHandler.Event` / `Action`. Callbacks are deferred: the terminal
//! queues `(character, tag, arg)` for the effect to handle after the tick.
use super::animation::SceneId;
use super::geometry::Coord;
use super::motion::PathId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    SegmentEntered,
    SegmentExited,
    PathActivated,
    PathComplete,
    PathHolding,
    SceneActivated,
    SceneComplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    Scene(SceneId),
    Path(PathId),
    /// (path, waypoint index)
    Waypoint(PathId, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    ActivatePath(PathId),
    ActivateScene(SceneId),
    DeactivatePath(Option<PathId>),
    DeactivateScene(Option<SceneId>),
    ResetAppearance,
    SetLayer(i32),
    SetCoordinate(Coord),
    /// The common `Callback(terminal.set_character_visibility, False)`.
    Hide,
    /// Any other callback: an effect-defined tag and argument.
    Callback(u32, i64),
}
