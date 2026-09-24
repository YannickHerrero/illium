//! Port of `terminaltexteffects.engine.animation` (release 0.15.0).
//! Frames are never moved between lists: `pos` splits played from pending.
use super::easing::Ease;
use super::graphics::{ColorPair, Gradient};

pub type SceneId = usize;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visual {
    pub symbol: char,
    pub colors: ColorPair,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMetric {
    Distance,
    Step,
}

#[derive(Clone, Debug)]
struct Frame {
    visual: Visual,
    duration: usize,
    ticks: usize,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub looping: bool,
    pub sync: Option<SyncMetric>,
    pub ease: Option<Ease>,
    frames: Vec<Frame>,
    pos: usize,
    index_map: Vec<usize>,
    easing_current_step: usize,
}

impl Scene {
    fn new(id: String, looping: bool, sync: Option<SyncMetric>, ease: Option<Ease>) -> Self {
        Self {
            id,
            looping,
            sync,
            ease,
            frames: vec![],
            pos: 0,
            index_map: vec![],
            easing_current_step: 0,
        }
    }
    pub fn add_frame(&mut self, symbol: char, duration: usize, colors: ColorPair) {
        assert!(duration >= 1, "frame duration must be at least 1");
        let index = self.frames.len();
        self.frames.push(Frame {
            visual: Visual { symbol, colors },
            duration,
            ticks: 0,
        });
        self.index_map.extend(std::iter::repeat_n(index, duration));
    }
    /// Frames still to play (`len(scene.frames)` in TTE).
    pub fn remaining(&self) -> usize {
        self.frames.len() - self.pos
    }
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn easing_total_steps(&self) -> usize {
        self.index_map.len()
    }
    fn activate(&self) -> Visual {
        self.frames
            .get(self.pos)
            .map(|f| f.visual)
            .expect("activated an empty scene")
    }
    fn next_visual(&mut self) -> Visual {
        let frame = &mut self.frames[self.pos];
        let visual = frame.visual;
        frame.ticks += 1;
        if frame.ticks == frame.duration {
            frame.ticks = 0;
            self.pos += 1;
            if self.looping && self.pos == self.frames.len() {
                self.pos = 0;
            }
        }
        visual
    }
    fn finish(&mut self) {
        self.pos = self.frames.len();
    }
    pub fn reset(&mut self) {
        for frame in &mut self.frames {
            frame.ticks = 0;
        }
        self.pos = 0;
        self.easing_current_step = 0;
    }
    pub fn apply_gradient_to_symbols(
        &mut self,
        symbols: &[char],
        duration: usize,
        fg: Option<&Gradient>,
        bg: Option<&Gradient>,
    ) {
        let fg = fg.filter(|g| !g.is_empty());
        let bg = bg.filter(|g| !g.is_empty());
        assert!(fg.is_some() || bg.is_some(), "a gradient must be provided");
        assert!(!symbols.is_empty());
        let pairs: Vec<ColorPair> = match (fg, bg) {
            (Some(f), Some(b)) if f.len() >= b.len() => cyclic(&f.spectrum, &b.spectrum)
                .into_iter()
                .map(|(f, b)| ColorPair::new(Some(f), Some(b)))
                .collect(),
            (Some(f), Some(b)) => cyclic(&b.spectrum, &f.spectrum)
                .into_iter()
                .map(|(b, f)| ColorPair::new(Some(f), Some(b)))
                .collect(),
            (Some(f), None) => f.spectrum.iter().map(|&c| ColorPair::fg(c)).collect(),
            (None, Some(b)) => b.spectrum.iter().map(|&c| ColorPair::bg(c)).collect(),
            (None, None) => unreachable!(),
        };
        if symbols.len() >= pairs.len() {
            for (symbol, colors) in cyclic(symbols, &pairs) {
                self.add_frame(symbol, duration, colors);
            }
        } else {
            for (colors, symbol) in cyclic(&pairs, symbols) {
                self.add_frame(symbol, duration, colors);
            }
        }
    }
}

/// `cyclic_distribution`: spreads the smaller sequence evenly over the larger.
fn cyclic<A: Copy, B: Copy>(larger: &[A], smaller: &[B]) -> Vec<(A, B)> {
    let repeat_factor = larger.len() / smaller.len();
    let mut overflow_count = larger.len() % smaller.len();
    let mut overflow_used = false;
    let mut index = 0;
    let mut current = 0;
    let mut out = Vec::with_capacity(larger.len());
    for &item in larger {
        if current >= repeat_factor {
            if overflow_count > 0 {
                if overflow_used {
                    index += 1;
                    current = 0;
                    overflow_used = false;
                } else {
                    overflow_used = true;
                    overflow_count -= 1;
                }
            } else {
                index += 1;
                current = 0;
            }
        }
        current += 1;
        out.push((item, smaller[index]));
    }
    out
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub scenes: Vec<Scene>,
    pub active: Option<SceneId>,
    pub current: Visual,
    input_symbol: char,
}

impl Animation {
    pub(crate) fn new(input_symbol: char) -> Self {
        Self {
            scenes: vec![],
            active: None,
            current: Visual {
                symbol: input_symbol,
                colors: ColorPair::default(),
            },
            input_symbol,
        }
    }
    pub fn new_scene(
        &mut self,
        looping: bool,
        sync: Option<SyncMetric>,
        ease: Option<Ease>,
        id: &str,
    ) -> SceneId {
        let id = if id.is_empty() {
            let mut n = self.scenes.len();
            while self.scenes.iter().any(|s| s.id == n.to_string()) {
                n += 1;
            }
            n.to_string()
        } else {
            id.to_string()
        };
        if let Some(existing) = self.scenes.iter().position(|s| s.id == id) {
            // TTE replaces the dict entry; the old Scene object stays referenced
            // by earlier registrations, so keep it and append the new one.
            self.scenes[existing].id.push('\u{0}');
        }
        self.scenes.push(Scene::new(id, looping, sync, ease));
        self.scenes.len() - 1
    }
    /// A default scene (`new_scene()` with no arguments).
    pub fn scene(&mut self) -> SceneId {
        self.new_scene(false, None, None, "")
    }
    pub fn named_scene(&mut self, id: &str) -> SceneId {
        self.new_scene(false, None, None, id)
    }
    pub fn query_scene(&self, id: &str) -> SceneId {
        self.scenes
            .iter()
            .position(|s| s.id == id)
            .unwrap_or_else(|| panic!("scene {id} not found"))
    }
    pub fn get(&mut self, scene: SceneId) -> &mut Scene {
        &mut self.scenes[scene]
    }
    pub fn active_scene_is_complete(&self) -> bool {
        match self.active {
            None => true,
            Some(s) => self.scenes[s].remaining() == 0 || self.scenes[s].looping,
        }
    }
    pub fn set_appearance(&mut self, symbol: Option<char>, colors: ColorPair) {
        self.current = Visual {
            symbol: symbol.unwrap_or(self.input_symbol),
            colors,
        };
    }
    pub fn deactivate_scene(&mut self, scene: Option<SceneId>) {
        match scene {
            None => self.active = None,
            Some(s) if self.active == Some(s) => self.active = None,
            Some(_) => {}
        }
    }
    pub(crate) fn activate(&mut self, scene: SceneId) {
        self.active = Some(scene);
        self.current = self.scenes[scene].activate();
    }
    /// Advances the active scene. `path` is the active path's
    /// (current_step, max_steps, total_distance, last_distance_reached).
    /// Returns the scene that completed on this step, if any.
    pub(crate) fn step(&mut self, path: Option<(i64, i64, f64, f64)>) -> Option<SceneId> {
        let id = self.active?;
        let scene = &mut self.scenes[id];
        if scene.remaining() == 0 {
            return None;
        }
        if let Some(sync) = scene.sync {
            match path {
                Some((current_step, max_steps, total, reached)) => {
                    let n = scene.remaining() as f64 - 1.0;
                    let ratio = match sync {
                        SyncMetric::Step => current_step.max(1) as f64 / max_steps.max(1) as f64,
                        SyncMetric::Distance => {
                            let total = total.max(1.0);
                            (total - (total - reached).max(1.0)).max(1.0) / total
                        }
                    };
                    let index = (n * ratio).round_ties_even() as usize;
                    let frames = &scene.frames[scene.pos..];
                    self.current = frames.get(index).unwrap_or(frames.last().unwrap()).visual;
                }
                None => {
                    self.current = scene.frames.last().unwrap().visual;
                    scene.finish();
                }
            }
        } else if let Some(ease) = scene.ease {
            let total = scene.easing_total_steps();
            let factor = ease.apply(scene.easing_current_step as f64 / total as f64);
            let index = (factor * total.saturating_sub(1) as f64).round_ties_even();
            let index = (index as i64).clamp(0, total as i64 - 1) as usize;
            self.current = scene.frames[scene.index_map[index]].visual;
            scene.easing_current_step += 1;
            if scene.easing_current_step == total {
                if scene.looping {
                    scene.easing_current_step = 0;
                } else {
                    scene.finish();
                }
            }
        } else {
            self.current = scene.next_visual();
        }
        if self.active_scene_is_complete() {
            if !self.scenes[id].looping {
                self.scenes[id].reset();
                self.active = None;
            }
            return Some(id);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::graphics::Color;
    #[test]
    fn cyclic_distribution_matches_python() {
        let pairs = cyclic(&[1, 2, 3, 4, 5], &['a', 'b']);
        assert_eq!(pairs, [(1, 'a'), (2, 'a'), (3, 'a'), (4, 'b'), (5, 'b')]);
    }
    #[test]
    fn scene_plays_then_completes() {
        let mut a = Animation::new('x');
        let s = a.scene();
        a.get(s).add_frame('a', 2, ColorPair::default());
        a.get(s)
            .add_frame('b', 1, ColorPair::fg(Color::hex("ffffff")));
        a.activate(s);
        assert_eq!(a.step(None), None);
        assert_eq!(a.current.symbol, 'a');
        assert_eq!(a.step(None), None);
        assert_eq!(a.step(None), Some(s));
        assert_eq!(a.current.symbol, 'b');
        assert!(a.active.is_none());
        assert_eq!(a.scenes[s].remaining(), 2);
    }
}
