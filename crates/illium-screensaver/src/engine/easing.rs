//! Port of `terminaltexteffects.utils.easing` (release 0.15.0).
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    InSine,
    OutSine,
    InOutSine,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
    /// `make_easing(x1, y1, x2, y2)`: a CSS-like cubic bezier.
    Bezier(f64, f64, f64, f64),
}

fn out_bounce(p: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if p < 1.0 / d1 {
        n1 * p * p
    } else if p < 2.0 / d1 {
        let p = p - 1.5 / d1;
        n1 * p * p + 0.75
    } else if p < 2.5 / d1 {
        let p = p - 2.25 / d1;
        n1 * p * p + 0.9375
    } else {
        let p = p - 2.625 / d1;
        n1 * p * p + 0.984375
    }
}

impl Ease {
    pub fn apply(self, p: f64) -> f64 {
        use Ease::*;
        match self {
            Linear => p,
            InSine => 1.0 - (p * PI / 2.0).cos(),
            OutSine => (p * PI / 2.0).sin(),
            InOutSine => -((PI * p).cos() - 1.0) / 2.0,
            InQuad => p * p,
            OutQuad => 1.0 - (1.0 - p) * (1.0 - p),
            InOutQuad => {
                if p < 0.5 {
                    2.0 * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(2) / 2.0
                }
            }
            InCubic => p.powi(3),
            OutCubic => 1.0 - (1.0 - p).powi(3),
            InOutCubic => {
                if p < 0.5 {
                    4.0 * p.powi(3)
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
                }
            }
            InQuart => p.powi(4),
            OutQuart => 1.0 - (1.0 - p).powi(4),
            InOutQuart => {
                if p < 0.5 {
                    8.0 * p.powi(4)
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(4) / 2.0
                }
            }
            InQuint => p.powi(5),
            OutQuint => 1.0 - (1.0 - p).powi(5),
            InOutQuint => {
                if p < 0.5 {
                    16.0 * p.powi(5)
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(5) / 2.0
                }
            }
            InExpo => {
                if p == 0.0 {
                    0.0
                } else {
                    2f64.powf(10.0 * p - 10.0)
                }
            }
            OutExpo => {
                if p == 1.0 {
                    1.0
                } else {
                    1.0 - 2f64.powf(-10.0 * p)
                }
            }
            InOutExpo => {
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else if p < 0.5 {
                    2f64.powf(20.0 * p - 10.0) / 2.0
                } else {
                    (2.0 - 2f64.powf(-20.0 * p + 10.0)) / 2.0
                }
            }
            InCirc => 1.0 - (1.0 - p * p).sqrt(),
            OutCirc => (1.0 - (p - 1.0).powi(2)).sqrt(),
            InOutCirc => {
                if p < 0.5 {
                    (1.0 - (1.0 - (2.0 * p).powi(2)).sqrt()) / 2.0
                } else {
                    ((1.0 - (-2.0 * p + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }
            InBack => {
                let c1 = 1.70158;
                (c1 + 1.0) * p.powi(3) - c1 * p * p
            }
            OutBack => {
                let c1 = 1.70158;
                1.0 + (c1 + 1.0) * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2)
            }
            InOutBack => {
                let c2 = 1.70158 * 1.525;
                if p < 0.5 {
                    (2.0 * p).powi(2) * ((c2 + 1.0) * 2.0 * p - c2) / 2.0
                } else {
                    ((2.0 * p - 2.0).powi(2) * ((c2 + 1.0) * (p * 2.0 - 2.0) + c2) + 2.0) / 2.0
                }
            }
            InElastic => {
                let c4 = 2.0 * PI / 3.0;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else {
                    -(2f64.powf(10.0 * p - 10.0)) * ((p * 10.0 - 10.75) * c4).sin()
                }
            }
            OutElastic => {
                let c4 = 2.0 * PI / 3.0;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else {
                    2f64.powf(-10.0 * p) * ((p * 10.0 - 0.75) * c4).sin() + 1.0
                }
            }
            InOutElastic => {
                let c5 = 2.0 * PI / 4.5;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else if p < 0.5 {
                    -(2f64.powf(20.0 * p - 10.0) * ((20.0 * p - 11.125) * c5).sin()) / 2.0
                } else {
                    2f64.powf(-20.0 * p + 10.0) * ((20.0 * p - 11.125) * c5).sin() / 2.0 + 1.0
                }
            }
            InBounce => 1.0 - out_bounce(1.0 - p),
            OutBounce => out_bounce(p),
            InOutBounce => {
                if p < 0.5 {
                    (1.0 - out_bounce(1.0 - 2.0 * p)) / 2.0
                } else {
                    (1.0 + out_bounce(2.0 * p - 1.0)) / 2.0
                }
            }
            Bezier(x1, y1, x2, y2) => {
                if p <= 0.0 {
                    return 0.0;
                }
                if p >= 1.0 {
                    return 1.0;
                }
                let x = |t: f64| {
                    3.0 * x1 * (1.0 - t).powi(2) * t + 3.0 * x2 * (1.0 - t) * t * t + t.powi(3)
                };
                let y = |t: f64| {
                    3.0 * y1 * (1.0 - t).powi(2) * t + 3.0 * y2 * (1.0 - t) * t * t + t.powi(3)
                };
                let dx = |t: f64| {
                    3.0 * (1.0 - t).powi(2) * x1
                        + 6.0 * (1.0 - t) * t * (x2 - x1)
                        + 3.0 * t * t * (1.0 - x2)
                };
                let mut t = p;
                for _ in 0..20 {
                    let error = x(t) - p;
                    if error.abs() < 1e-5 {
                        break;
                    }
                    let d = dx(t);
                    if d.abs() < 1e-6 {
                        break;
                    }
                    t -= error / d;
                }
                y(t)
            }
        }
    }
}

/// `EasingTracker`: steps an easing function over a fixed number of steps.
#[derive(Clone, Debug)]
pub struct EasingTracker {
    pub ease: Ease,
    pub total_steps: usize,
    clamp: bool,
    pub current_step: usize,
    pub progress_ratio: f64,
    pub step_delta: f64,
    pub eased_value: f64,
    last_eased_value: f64,
}

impl EasingTracker {
    pub fn new(ease: Ease, total_steps: usize, clamp: bool) -> Self {
        Self {
            ease,
            total_steps,
            clamp,
            current_step: 0,
            progress_ratio: 0.0,
            step_delta: 0.0,
            eased_value: 0.0,
            last_eased_value: 0.0,
        }
    }
    pub fn step(&mut self) -> f64 {
        if self.current_step < self.total_steps {
            self.current_step += 1;
            self.progress_ratio = self.current_step as f64 / self.total_steps as f64;
            self.eased_value = self.ease.apply(self.progress_ratio);
            if self.clamp {
                self.eased_value = self.eased_value.clamp(0.0, 1.0);
            }
            self.step_delta = self.eased_value - self.last_eased_value;
            self.last_eased_value = self.eased_value;
        }
        self.eased_value
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.ease, self.total_steps, self.clamp);
    }
    pub fn is_complete(&self) -> bool {
        self.current_step >= self.total_steps
    }
}

/// `SequenceEaser`: reveals a sequence over eased steps; `added`/`removed`
/// are ranges into the sequence for the last step.
#[derive(Clone, Debug)]
pub struct SequenceEaser<T: Clone> {
    pub sequence: Vec<T>,
    pub tracker: EasingTracker,
    pub added: Vec<T>,
    pub removed: Vec<T>,
    pub total: Vec<T>,
}

impl<T: Clone> SequenceEaser<T> {
    pub fn new(sequence: Vec<T>, ease: Ease, total_steps: usize) -> Self {
        Self {
            sequence,
            tracker: EasingTracker::new(ease, total_steps, true),
            added: vec![],
            removed: vec![],
            total: vec![],
        }
    }
    pub fn step(&mut self) -> &[T] {
        let previous_eased = self.tracker.eased_value;
        let eased = self.tracker.step();
        let n = self.sequence.len();
        if n == 0 {
            self.added.clear();
            self.removed.clear();
            self.total.clear();
            return &self.added;
        }
        let length = (eased * n as f64) as usize;
        let previous = (previous_eased * n as f64) as usize;
        self.added.clear();
        self.removed.clear();
        if length > previous {
            self.added
                .extend_from_slice(&self.sequence[previous..length]);
        } else if length < previous {
            self.removed
                .extend_from_slice(&self.sequence[length..previous]);
        }
        self.total = self.sequence[..length].to_vec();
        &self.added
    }
    pub fn is_complete(&self) -> bool {
        self.tracker.is_complete()
    }
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.added.clear();
        self.removed.clear();
        self.total.clear();
    }
}
