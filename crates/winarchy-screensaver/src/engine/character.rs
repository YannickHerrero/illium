//! Port of `terminaltexteffects.engine.base_character` (release 0.15.0).
use super::animation::{Animation, SceneId};
use super::event::{Action, Caller, Event};
use super::geometry::Coord;
use super::motion::{Motion, PathId};

pub type CharId = usize;

#[derive(Clone, Debug)]
pub struct Character {
    pub id: CharId,
    pub input_symbol: char,
    pub input_coord: Coord,
    pub visible: bool,
    pub layer: i32,
    pub is_fill: bool,
    pub animation: Animation,
    pub motion: Motion,
    events: Vec<(Event, Caller, Action)>,
    /// Callbacks fired since the terminal last collected them.
    pub(crate) fired: Vec<(u32, i64)>,
}

impl Character {
    pub(crate) fn new(id: CharId, symbol: char, coord: Coord) -> Self {
        Self {
            id,
            input_symbol: symbol,
            input_coord: coord,
            visible: false,
            layer: 0,
            is_fill: false,
            animation: Animation::new(symbol),
            motion: Motion::new(coord),
            events: vec![],
            fired: vec![],
        }
    }
    pub(crate) fn set_input_coord(&mut self, coord: Coord) {
        self.input_coord = coord;
        self.motion.set_coordinate(coord);
    }
    pub fn register(&mut self, event: Event, caller: Caller, action: Action) {
        let entry = (event, caller, action);
        assert!(
            !self.events.contains(&entry),
            "duplicate event registration"
        );
        self.events.push(entry);
    }
    /// `event_handler.registered_events.clear()`.
    pub fn clear_events(&mut self) {
        self.events.clear();
    }
    pub fn is_active(&self) -> bool {
        !self.animation.active_scene_is_complete() || !self.motion.movement_is_complete()
    }
    pub fn activate_scene(&mut self, scene: SceneId) {
        self.animation.activate(scene);
        self.handle_event(Event::SceneActivated, Caller::Scene(scene));
    }
    pub fn activate_path(&mut self, path: PathId) {
        if let Some(layer) = self.motion.activate(path) {
            self.layer = layer;
        }
        self.handle_event(Event::PathActivated, Caller::Path(path));
    }
    pub fn chain_paths(&mut self, paths: &[PathId], looping: bool) {
        if paths.len() < 2 {
            return;
        }
        for pair in paths.windows(2) {
            self.register(
                Event::PathComplete,
                Caller::Path(pair[0]),
                Action::ActivatePath(pair[1]),
            );
        }
        if looping {
            self.register(
                Event::PathComplete,
                Caller::Path(*paths.last().unwrap()),
                Action::ActivatePath(paths[0]),
            );
        }
    }
    fn handle_event(&mut self, event: Event, caller: Caller) {
        let actions: Vec<Action> = self
            .events
            .iter()
            .filter(|(e, c, _)| *e == event && *c == caller)
            .map(|&(_, _, a)| a)
            .collect();
        for action in actions {
            match action {
                Action::ActivatePath(p) => self.activate_path(p),
                Action::ActivateScene(s) => self.activate_scene(s),
                Action::DeactivatePath(p) => self.motion.deactivate_path(p),
                Action::DeactivateScene(s) => self.animation.deactivate_scene(s),
                Action::ResetAppearance => self
                    .animation
                    .set_appearance(Some(self.input_symbol), Default::default()),
                Action::SetLayer(layer) => self.layer = layer,
                Action::SetCoordinate(coord) => self.motion.current_coord = coord,
                Action::Hide => self.visible = false,
                Action::Callback(tag, arg) => self.fired.push((tag, arg)),
            }
        }
    }
    fn move_(&mut self) {
        self.motion.previous_coord = self.motion.current_coord;
        let mut events = vec![];
        let Some(next) = self.motion.step_active(&mut events) else {
            return;
        };
        for (event, caller) in events {
            self.handle_event(event, caller);
        }
        self.motion.current_coord = next;
        let Some(id) = self.motion.active else {
            return;
        };
        let path = &mut self.motion.paths[id];
        if path.current_step != path.max_steps {
            return;
        }
        if path.hold_time > 0 && path.hold_time_remaining == path.hold_time {
            self.handle_event(Event::PathHolding, Caller::Path(id));
            if let Some(active) = self.motion.active {
                let path = &mut self.motion.paths[active];
                path.hold_time_remaining = path.hold_time_remaining.saturating_sub(1);
            }
            return;
        }
        if path.hold_time_remaining > 0 {
            path.hold_time_remaining -= 1;
            return;
        }
        // `len(segments) > 1`: the origin segment plus at least one more.
        if path.looping && path.waypoints.len() > 1 {
            self.motion.deactivate_path(Some(id));
            self.activate_path(id);
        } else {
            self.motion.deactivate_path(Some(id));
            self.handle_event(Event::PathComplete, Caller::Path(id));
        }
    }
    pub fn tick(&mut self) {
        self.move_();
        let progress = self.motion.active_progress();
        if let Some(scene) = self.animation.step(progress) {
            self.handle_event(Event::SceneComplete, Caller::Scene(scene));
        }
    }
}
