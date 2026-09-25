//! Pure lock-idle state; the animation is `illium_screensaver`'s player,
//! one per monitor like Omarchy's one terminal per monitor.
use illium_config::screensaver::Screensaver;
use illium_screensaver::player::Player;
use std::time::{Duration, Instant};

pub struct Saver {
    config: Screensaver,
    activity: u64,
    idle_since: Instant,
    pub saving: bool,
    players: Vec<Player>,
    last_frame: Instant,
    seed: u64,
}

/// A monitor surface in physical pixels and its DPI scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub width: usize,
    pub height: usize,
    pub scale: f32,
}

impl Saver {
    pub fn new(config: Screensaver, now: Instant, activity: u64, seed: u64) -> Self {
        Self {
            config,
            activity,
            idle_since: now,
            saving: false,
            players: vec![],
            last_frame: now,
            seed,
        }
    }
    /// Losing the input desktop must not keep an invisible animation running.
    pub fn reset(&mut self, now: Instant, activity: u64) {
        self.activity = activity;
        self.idle_since = now;
        self.saving = false;
        self.players.clear();
    }
    /// Returns whether the visible mode changed. Activity always wins over timeout.
    pub fn poll(&mut self, now: Instant, activity: u64) -> bool {
        if activity != self.activity {
            self.activity = activity;
            self.idle_since = now;
            let was = self.saving;
            self.saving = false;
            self.players.clear();
            return was;
        }
        if !self.config.enabled || self.config.effects.is_empty() || self.saving {
            return false;
        }
        if now.duration_since(self.idle_since) >= Duration::from_secs(self.config.timeout) {
            self.saving = true;
            self.last_frame = now;
            return true;
        }
        false
    }
    /// Advances every monitor's animation; returns, per surface, the new
    /// RGB frame when it changed.
    pub fn frames(&mut self, now: Instant, surfaces: &[Surface]) -> Vec<Option<&[u8]>> {
        if !self.saving {
            return vec![None; surfaces.len()];
        }
        let elapsed = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        self.players.truncate(surfaces.len());
        let mut changed = vec![];
        for (index, surface) in surfaces.iter().enumerate() {
            let size = (surface.width, surface.height);
            if self.players.get(index).is_none_or(|p| p.size() != size) {
                self.seed = self
                    .seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let player = Player::new(
                    &self.config.effects,
                    surface.width,
                    surface.height,
                    surface.scale,
                    self.seed,
                );
                if index < self.players.len() {
                    self.players[index] = player;
                } else {
                    self.players.push(player);
                }
                changed.push(true);
            } else {
                changed.push(self.players[index].advance(elapsed));
            }
        }
        self.players
            .iter()
            .zip(changed)
            .map(|(p, changed)| changed.then(|| p.pixels()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use illium_config::screensaver::Effect;
    const SURFACE: Surface = Surface {
        width: 640,
        height: 360,
        scale: 1.0,
    };
    #[test]
    fn idle_and_activity() {
        let now = Instant::now();
        let mut s = Saver::new(Screensaver::default(), now, 0, 42);
        assert!(!s.poll(now + Duration::from_secs(29), 0));
        assert!(s.poll(now + Duration::from_secs(30), 0));
        assert!(s.saving);
        assert!(!s.poll(now + Duration::from_secs(42), 0));
        assert!(s.poll(now + Duration::from_secs(43), 1));
        assert!(!s.saving);
        assert!(!s.poll(now + Duration::from_secs(72), 1));
        assert!(!s.poll(now + Duration::from_secs(73), 2));
        assert!(!s.saving);
    }
    #[test]
    fn disabled_never_saves() {
        let now = Instant::now();
        let c = Screensaver {
            enabled: false,
            ..Screensaver::default()
        };
        let mut s = Saver::new(c, now, 0, 0);
        assert!(!s.poll(now + Duration::from_secs(100), 0));
        assert!(s.frames(now, &[SURFACE]).iter().all(Option::is_none));
    }
    #[test]
    fn reset_restarts_idle() {
        let now = Instant::now();
        let mut s = Saver::new(Screensaver::default(), now, 0, 7);
        s.poll(now + Duration::from_secs(30), 0);
        s.reset(now + Duration::from_secs(31), 2);
        assert!(!s.saving);
        assert!(!s.poll(now + Duration::from_secs(60), 2));
        assert!(s.poll(now + Duration::from_secs(61), 2));
    }
    #[test]
    fn frames_follow_each_surface() {
        let now = Instant::now();
        let c = Screensaver {
            effects: vec![Effect::Decrypt],
            ..Screensaver::default()
        };
        let mut s = Saver::new(c, now, 0, 7);
        assert!(s.poll(now + Duration::from_secs(30), 0));
        let small = Surface {
            width: 320,
            height: 200,
            scale: 1.0,
        };
        let first = s.frames(now + Duration::from_secs(30), &[SURFACE, small]);
        assert_eq!(first[0].map(<[u8]>::len), Some(640 * 360 * 3));
        assert_eq!(first[1].map(<[u8]>::len), Some(320 * 200 * 3));
        let later = s.frames(now + Duration::from_secs(33), &[SURFACE]);
        assert_eq!(later.len(), 1);
        assert!(later[0].is_some(), "decrypt draws during its first seconds");
    }
}
