//! Pure lock-idle state and bounded ASCII animation frames (no desktop required).
use std::time::{Duration, Instant};
use winarchy_config::screensaver::{Effect, Screensaver};

pub struct Saver {
    config: Screensaver,
    activity: u64,
    idle_since: Instant,
    cycle: Instant,
    pub effect: Option<Effect>,
    previous: Option<Effect>,
    random: u64,
}
impl Saver {
    pub fn new(config: Screensaver, now: Instant, activity: u64, seed: u64) -> Self {
        Self {
            config,
            activity,
            idle_since: now,
            cycle: now,
            effect: None,
            previous: None,
            random: seed.max(1),
        }
    }
    /// Losing the input desktop must not keep an invisible animation running.
    pub fn reset(&mut self, now: Instant, activity: u64) {
        self.activity = activity;
        self.idle_since = now;
        self.effect = None;
    }
    /// Returns whether the visible mode changed. Activity always wins over timeout.
    pub fn poll(&mut self, now: Instant, activity: u64) -> bool {
        if activity != self.activity {
            self.activity = activity;
            self.idle_since = now;
            return self.effect.take().is_some();
        }
        if !self.config.enabled || self.config.effects.is_empty() {
            return false;
        }
        if self.effect.is_none()
            && now.duration_since(self.idle_since) >= Duration::from_secs(self.config.timeout)
        {
            self.choose(now);
            return true;
        }
        if self.effect.is_some() && now.duration_since(self.cycle) >= Duration::from_secs(12) {
            self.choose(now);
        }
        false
    }
    fn choose(&mut self, now: Instant) {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        let choices: Vec<_> = self
            .config
            .effects
            .iter()
            .copied()
            .filter(|e| self.config.effects.len() == 1 || Some(*e) != self.previous)
            .collect();
        self.effect = Some(choices[self.random as usize % choices.len()]);
        self.previous = self.effect;
        self.cycle = now;
    }
    pub fn frame(&self, now: Instant, monitor: usize) -> Vec<Row> {
        frame(
            self.effect.unwrap_or(Effect::Decrypt),
            now.duration_since(self.cycle).as_secs_f32(),
            monitor,
        )
    }
}

pub const WIDTH: usize = 64;
pub const HEIGHT: usize = 23;
const LOGO: [&str; 5] = [
    "W   W III N   N  AAA  RRRR   CCC H   H Y   Y",
    "W   W  I  NN  N A   A R   R C    H   H  Y Y ",
    "W W W  I  N N N AAAAA RRRR  C    HHHHH   Y  ",
    "WW WW  I  N  NN A   A R  R  C    H   H   Y  ",
    "W   W III N   N A   A R   R  CCC H   H   Y  ",
];
pub struct Row {
    pub text: String,
    pub intensity: f32,
    pub blend: f32,
}
fn noise(x: usize, y: usize, tick: usize) -> usize {
    let mut n = (x as u64).wrapping_mul(0x9e3779b9)
        ^ (y as u64).wrapping_mul(0x85ebca6b)
        ^ (tick as u64).wrapping_mul(0xc2b2ae35);
    n ^= n >> 16;
    n = n.wrapping_mul(0x45d9f3b);
    (n ^ (n >> 16)) as usize
}
fn logo(x: usize, y: usize) -> char {
    let left = (WIDTH - LOGO[0].len()) / 2;
    if (9..14).contains(&y) && x >= left {
        LOGO[y - 9]
            .as_bytes()
            .get(x - left)
            .copied()
            .unwrap_or(b' ') as char
    } else {
        ' '
    }
}
pub fn frame(effect: Effect, seconds: f32, monitor: usize) -> Vec<Row> {
    let tick = (seconds * 15.0) as usize;
    let glyphs = b"0123456789ABCDEF:.*+";
    (0..HEIGHT)
        .map(|y| {
            let beam = ((y as f32 - (seconds * 5.0) % (HEIGHT as f32 + 8.0)).abs() / 5.0).min(1.0);
            let text = (0..WIDTH)
                .map(|x| {
                    let target = logo(x, y);
                    let n = noise(x, y + monitor * HEIGHT, tick);
                    match effect {
                        Effect::Decrypt
                            if target != ' '
                                && seconds < 7.0
                                && noise(x, y, monitor) % 100
                                    > (seconds / 7.0 * 100.0) as usize =>
                        {
                            glyphs[n % glyphs.len()] as char
                        }
                        Effect::Matrix => {
                            let head = (tick + noise(x, 0, monitor) % (HEIGHT * 2)) % (HEIGHT * 2);
                            let distance = (head + HEIGHT * 2 - y) % (HEIGHT * 2);
                            if distance < 7 {
                                glyphs[n % glyphs.len()] as char
                            } else if seconds > 2.0 {
                                target
                            } else {
                                ' '
                            }
                        }
                        _ => target,
                    }
                })
                .collect();
            Row {
                text,
                intensity: if effect == Effect::Beams {
                    0.2 + 0.8 * (1.0 - beam)
                } else {
                    0.85
                },
                blend: ((seconds * 0.7 + y as f32 * 0.22 + monitor as f32).sin() + 1.0) * 0.5,
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_activity_and_cycles() {
        let now = Instant::now();
        let mut s = Saver::new(Screensaver::default(), now, 0, 42);
        assert!(!s.poll(now + Duration::from_secs(29), 0));
        assert!(s.poll(now + Duration::from_secs(30), 0));
        let first = s.effect;
        s.poll(now + Duration::from_secs(42), 0);
        assert_ne!(s.effect, first);
        assert!(s.poll(now + Duration::from_secs(43), 1));
        assert_eq!(s.effect, None);
        assert!(!s.poll(now + Duration::from_secs(72), 1));
        assert!(!s.poll(now + Duration::from_secs(73), 2));
        assert_eq!(s.effect, None);
    }
    #[test]
    fn disabled_and_single_effect() {
        let now = Instant::now();
        let mut c = Screensaver {
            enabled: false,
            ..Screensaver::default()
        };
        let mut s = Saver::new(c.clone(), now, 0, 0);
        assert!(!s.poll(now + Duration::from_secs(100), 0));
        c.enabled = true;
        c.effects = vec![Effect::Beams];
        let mut s = Saver::new(c, now, 0, 0);
        for t in [30, 42, 54] {
            s.poll(now + Duration::from_secs(t), 0);
            assert_eq!(s.effect, Some(Effect::Beams));
        }
    }
    #[test]
    fn reset_restarts_idle_and_previous_effect_survives_wake() {
        let now = Instant::now();
        let mut s = Saver::new(Screensaver::default(), now, 0, 7);
        s.poll(now + Duration::from_secs(30), 0);
        let first = s.effect;
        s.reset(now + Duration::from_secs(31), 2);
        assert_eq!(s.effect, None);
        assert!(!s.poll(now + Duration::from_secs(60), 2));
        assert!(s.poll(now + Duration::from_secs(61), 2));
        assert_ne!(s.effect, first);
        for t in (73..1000).step_by(12) {
            let previous = s.effect;
            s.poll(now + Duration::from_secs(t), 2);
            assert_ne!(s.effect, previous);
        }
    }
    #[test]
    fn decrypt_finishes_and_rain_moves() {
        let complete = frame(Effect::Decrypt, 8.0, 0);
        let logo = frame(Effect::ColorShift, 8.0, 0);
        assert!(complete.iter().zip(&logo).all(|(a, b)| a.text == b.text));
        assert!(
            frame(Effect::Decrypt, 0.0, 0)
                .iter()
                .zip(&logo)
                .any(|(a, b)| a.text != b.text)
        );
        assert!(
            frame(Effect::Matrix, 0.0, 0)
                .iter()
                .zip(frame(Effect::Matrix, 1.0, 0))
                .any(|(a, b)| a.text != b.text)
        );
        assert!(
            frame(Effect::Beams, 0.0, 0)
                .iter()
                .zip(frame(Effect::Beams, 2.0, 0))
                .any(|(a, b)| a.intensity != b.intensity)
        );
    }
    #[test]
    fn frames_are_bounded_and_animated() {
        for effect in Screensaver::default().effects {
            let a = frame(effect, 0.0, 0);
            let b = frame(effect, 4.0, 1);
            assert_eq!(a.len(), HEIGHT);
            assert!(
                a.iter()
                    .all(|r| r.text.len() == WIDTH && (0.0..=1.0).contains(&r.blend))
            );
            assert!(
                a.iter()
                    .zip(b)
                    .any(|(a, b)| a.text != b.text || a.blend != b.blend)
            );
        }
    }
}
