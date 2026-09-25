//! Port of `terminaltexteffects.engine.motion` (release 0.15.0).
use super::easing::Ease;
use super::event::{Caller, Event};
use super::geometry::{
    Coord, find_coord_on_bezier_curve, find_coord_on_line, find_length_of_bezier_curve,
    find_length_of_line,
};

pub type PathId = usize;

#[derive(Clone, Debug)]
pub struct Waypoint {
    pub id: String,
    pub coord: Coord,
    pub bezier: Vec<Coord>,
}

#[derive(Clone, Debug)]
struct Segment {
    start: Coord,
    /// Index of the end waypoint in `Path::waypoints`.
    end: usize,
    distance: f64,
    entered: bool,
    exited: bool,
}

#[derive(Clone, Debug)]
pub struct Path {
    pub id: String,
    pub speed: f64,
    pub ease: Option<Ease>,
    pub layer: Option<i32>,
    pub hold_time: usize,
    pub looping: bool,
    pub waypoints: Vec<Waypoint>,
    segments: Vec<Segment>,
    pub total_distance: f64,
    pub current_step: i64,
    pub max_steps: i64,
    pub hold_time_remaining: usize,
    pub last_distance_reached: f64,
    origin: Option<f64>,
}

fn distance(from: Coord, to: &Waypoint) -> f64 {
    if to.bezier.is_empty() {
        find_length_of_line(from, to.coord, true)
    } else {
        find_length_of_bezier_curve(from, &to.bezier, to.coord)
    }
}

impl Path {
    fn new(
        id: String,
        speed: f64,
        ease: Option<Ease>,
        layer: Option<i32>,
        hold_time: usize,
        looping: bool,
    ) -> Self {
        assert!(speed > 0.0, "path speed must be positive");
        Self {
            id,
            speed,
            ease,
            layer,
            hold_time,
            looping,
            waypoints: vec![],
            segments: vec![],
            total_distance: 0.0,
            current_step: 0,
            max_steps: 0,
            hold_time_remaining: hold_time,
            last_distance_reached: 0.0,
            origin: None,
        }
    }
    /// Adds a waypoint; returns its index (the `Caller::Waypoint` key).
    pub fn new_waypoint(&mut self, coord: Coord, bezier: &[Coord], id: &str) -> usize {
        let id = if id.is_empty() {
            let mut n = self.waypoints.len();
            while self.waypoints.iter().any(|w| w.id == n.to_string()) {
                n += 1;
            }
            n.to_string()
        } else {
            id.to_string()
        };
        self.waypoints.push(Waypoint {
            id,
            coord,
            bezier: bezier.to_vec(),
        });
        let index = self.waypoints.len() - 1;
        if index >= 1 {
            let start = self.waypoints[index - 1].coord;
            let d = distance(start, &self.waypoints[index]);
            self.total_distance += d;
            self.segments.push(Segment {
                start,
                end: index,
                distance: d,
                entered: false,
                exited: false,
            });
            self.max_steps = (self.total_distance / self.speed).round_ties_even() as i64;
        }
        index
    }
    pub fn waypoint(&mut self, coord: Coord) -> usize {
        self.new_waypoint(coord, &[], "")
    }
    pub fn query_waypoint(&self, id: &str) -> usize {
        self.waypoints
            .iter()
            .position(|w| w.id == id)
            .unwrap_or_else(|| panic!("waypoint {id} not found"))
    }
    fn end_coord(&self) -> Coord {
        self.waypoints[self.segments.last().unwrap().end].coord
    }
    fn step(&mut self, id: PathId, events: &mut Vec<(Event, Caller)>) -> Coord {
        if self.max_steps == 0 || self.current_step >= self.max_steps || self.total_distance == 0.0
        {
            return self.end_coord();
        }
        self.current_step += 1;
        let ratio = self.current_step as f64 / self.max_steps as f64;
        let factor = self.ease.map_or(ratio, |e| e.apply(ratio));
        let mut to_travel = factor * self.total_distance;
        self.last_distance_reached = to_travel;
        let mut active = None;
        for (i, segment) in self.segments.iter_mut().enumerate() {
            if to_travel <= segment.distance {
                active = Some(i);
                if !segment.entered {
                    segment.entered = true;
                    events.push((Event::SegmentEntered, Caller::Waypoint(id, segment.end)));
                }
                break;
            }
            to_travel -= segment.distance;
            if !segment.exited {
                segment.exited = true;
                events.push((Event::SegmentExited, Caller::Waypoint(id, segment.end)));
            }
        }
        let active = active.unwrap_or_else(|| {
            let last = self.segments.len() - 1;
            to_travel += self.segments[last].distance;
            last
        });
        let segment = &self.segments[active];
        let t = if segment.distance == 0.0 {
            0.0
        } else if self.ease.is_some() {
            to_travel / segment.distance
        } else {
            (to_travel / segment.distance).min(1.0)
        };
        let end = &self.waypoints[segment.end];
        if end.bezier.is_empty() {
            find_coord_on_line(segment.start, end.coord, t)
        } else {
            find_coord_on_bezier_curve(segment.start, &end.bezier, end.coord, t)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Motion {
    pub paths: Vec<Path>,
    pub active: Option<PathId>,
    pub current_coord: Coord,
    pub previous_coord: Coord,
}

impl Motion {
    pub(crate) fn new(coord: Coord) -> Self {
        Self {
            paths: vec![],
            active: None,
            current_coord: coord,
            previous_coord: Coord::new(-1, -1),
        }
    }
    pub fn new_path(
        &mut self,
        speed: f64,
        ease: Option<Ease>,
        layer: Option<i32>,
        hold_time: usize,
        looping: bool,
        id: &str,
    ) -> PathId {
        let id = if id.is_empty() {
            let mut n = self.paths.len();
            while self.paths.iter().any(|p| p.id == n.to_string()) {
                n += 1;
            }
            n.to_string()
        } else {
            assert!(
                !self.paths.iter().any(|p| p.id == id),
                "duplicate path id {id}"
            );
            id.to_string()
        };
        self.paths
            .push(Path::new(id, speed, ease, layer, hold_time, looping));
        self.paths.len() - 1
    }
    /// `new_path(speed=..., ease=...)` with the other arguments defaulted.
    pub fn path(&mut self, speed: f64, ease: Option<Ease>) -> PathId {
        self.new_path(speed, ease, None, 0, false, "")
    }
    pub fn query_path(&self, id: &str) -> PathId {
        self.paths
            .iter()
            .position(|p| p.id == id)
            .unwrap_or_else(|| panic!("path {id} not found"))
    }
    pub fn get(&mut self, path: PathId) -> &mut Path {
        &mut self.paths[path]
    }
    pub fn set_coordinate(&mut self, coord: Coord) {
        self.current_coord = coord;
    }
    pub fn movement_is_complete(&self) -> bool {
        self.active.is_none()
    }
    pub fn deactivate_path(&mut self, path: Option<PathId>) {
        match path {
            None => self.active = None,
            Some(p) if self.active == Some(p) => self.active = None,
            Some(_) => {}
        }
    }
    /// Activation bookkeeping; the caller fires `PathActivated` and applies the layer.
    pub(crate) fn activate(&mut self, id: PathId) -> Option<i32> {
        let current = self.current_coord;
        let path = &mut self.paths[id];
        assert!(!path.waypoints.is_empty(), "activated an empty path");
        self.active = Some(id);
        let d = distance(current, &path.waypoints[0]);
        path.total_distance += d;
        if let Some(previous) = path.origin {
            path.segments.remove(0);
            path.total_distance -= previous;
        }
        path.origin = Some(d);
        path.segments.insert(
            0,
            Segment {
                start: current,
                end: 0,
                distance: d,
                entered: false,
                exited: false,
            },
        );
        path.current_step = 0;
        path.hold_time_remaining = path.hold_time;
        path.max_steps = (path.total_distance / path.speed).round_ties_even() as i64;
        for segment in &mut path.segments {
            segment.entered = false;
            segment.exited = false;
        }
        path.layer
    }
    pub(crate) fn step_active(&mut self, events: &mut Vec<(Event, Caller)>) -> Option<Coord> {
        let id = self.active?;
        if self.paths[id].segments.is_empty() {
            return None;
        }
        Some(self.paths[id].step(id, events))
    }
    pub(crate) fn active_progress(&self) -> Option<(i64, i64, f64, f64)> {
        self.active.map(|id| {
            let p = &self.paths[id];
            (
                p.current_step,
                p.max_steps,
                p.total_distance,
                p.last_distance_reached,
            )
        })
    }
}
