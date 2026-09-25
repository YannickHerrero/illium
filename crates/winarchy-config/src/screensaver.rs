//! Optional lock-surface animation settings in winarchy.toml.
use serde::Deserialize;

/// TerminalTextEffects effect names, as `tte` spells them.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Beams,
    BinaryPath,
    Blackhole,
    BouncyBalls,
    Bubbles,
    Burn,
    #[serde(alias = "color-shift")]
    ColorShift,
    Crumble,
    Decrypt,
    ErrorCorrect,
    Expand,
    Fireworks,
    Highlight,
    LaserEtch,
    Matrix,
    MiddleOut,
    OrbittingVolley,
    Overflow,
    Pour,
    Print,
    Rain,
    RandomSequence,
    Rings,
    Scattered,
    Slice,
    Slide,
    Smoke,
    Spotlights,
    Spray,
    Swarm,
    Sweep,
    SynthGrid,
    Thunderstorm,
    Unstable,
    VhsTape,
    Waves,
    Wipe,
}
impl Effect {
    /// Every effect, in `tte` order (the default: Omarchy uses `--random-effect`).
    pub const ALL: &[Effect] = &[
        Effect::Beams,
        Effect::BinaryPath,
        Effect::Blackhole,
        Effect::BouncyBalls,
        Effect::Bubbles,
        Effect::Burn,
        Effect::ColorShift,
        Effect::Crumble,
        Effect::Decrypt,
        Effect::ErrorCorrect,
        Effect::Expand,
        Effect::Fireworks,
        Effect::Highlight,
        Effect::LaserEtch,
        Effect::Matrix,
        Effect::MiddleOut,
        Effect::OrbittingVolley,
        Effect::Overflow,
        Effect::Pour,
        Effect::Print,
        Effect::Rain,
        Effect::RandomSequence,
        Effect::Rings,
        Effect::Scattered,
        Effect::Slice,
        Effect::Slide,
        Effect::Smoke,
        Effect::Spotlights,
        Effect::Spray,
        Effect::Swarm,
        Effect::Sweep,
        Effect::SynthGrid,
        Effect::Thunderstorm,
        Effect::Unstable,
        Effect::VhsTape,
        Effect::Waves,
        Effect::Wipe,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Effect::Beams => "beams",
            Effect::BinaryPath => "binarypath",
            Effect::Blackhole => "blackhole",
            Effect::BouncyBalls => "bouncyballs",
            Effect::Bubbles => "bubbles",
            Effect::Burn => "burn",
            Effect::ColorShift => "colorshift",
            Effect::Crumble => "crumble",
            Effect::Decrypt => "decrypt",
            Effect::ErrorCorrect => "errorcorrect",
            Effect::Expand => "expand",
            Effect::Fireworks => "fireworks",
            Effect::Highlight => "highlight",
            Effect::LaserEtch => "laseretch",
            Effect::Matrix => "matrix",
            Effect::MiddleOut => "middleout",
            Effect::OrbittingVolley => "orbittingvolley",
            Effect::Overflow => "overflow",
            Effect::Pour => "pour",
            Effect::Print => "print",
            Effect::Rain => "rain",
            Effect::RandomSequence => "randomsequence",
            Effect::Rings => "rings",
            Effect::Scattered => "scattered",
            Effect::Slice => "slice",
            Effect::Slide => "slide",
            Effect::Smoke => "smoke",
            Effect::Spotlights => "spotlights",
            Effect::Spray => "spray",
            Effect::Swarm => "swarm",
            Effect::Sweep => "sweep",
            Effect::SynthGrid => "synthgrid",
            Effect::Thunderstorm => "thunderstorm",
            Effect::Unstable => "unstable",
            Effect::VhsTape => "vhstape",
            Effect::Waves => "waves",
            Effect::Wipe => "wipe",
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "Raw")]
pub struct Screensaver {
    pub enabled: bool,
    pub timeout: u64,
    pub effects: Vec<Effect>,
}
impl Default for Screensaver {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: 30,
            effects: Effect::ALL.to_vec(),
        }
    }
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Raw {
    enabled: bool,
    timeout: u64,
    effects: Vec<Effect>,
}
impl Default for Raw {
    fn default() -> Self {
        let c = Screensaver::default();
        Self {
            enabled: c.enabled,
            timeout: c.timeout,
            effects: c.effects,
        }
    }
}
impl TryFrom<Raw> for Screensaver {
    type Error = String;
    fn try_from(raw: Raw) -> Result<Self, String> {
        if !(1..=86400).contains(&raw.timeout) {
            return Err("screensaver timeout must be 1..=86400 seconds".into());
        }
        let mut effects = Vec::new();
        for effect in raw.effects {
            if !effects.contains(&effect) {
                effects.push(effect);
            }
        }
        if effects.is_empty() {
            return Err("screensaver effects must not be empty".into());
        }
        Ok(Self {
            enabled: raw.enabled,
            timeout: raw.timeout,
            effects,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_global_config_and_disabled_settings() {
        let global: crate::config::Global = toml::from_str("theme = 'nord'").unwrap();
        assert_eq!(global.screensaver.timeout, 30);
        let global: crate::config::Global = toml::from_str("theme = 'nord'\n[screensaver]\nenabled = false\ntimeout = 10\neffects = ['color-shift']").unwrap();
        assert!(!global.screensaver.enabled);
        assert_eq!(global.screensaver.effects, vec![Effect::ColorShift]);
    }
    #[test]
    fn defaults_and_validation() {
        let c: Screensaver = toml::from_str("").unwrap();
        assert!(c.enabled);
        assert_eq!(c.timeout, 30);
        assert_eq!(c.effects, Effect::ALL);
        let c: Screensaver = toml::from_str("effects = ['colorshift', 'color-shift']").unwrap();
        assert_eq!(c.effects, vec![Effect::ColorShift]);
        for bad in [
            "timeout = 0",
            "timeout = -1",
            "timeout = 86401",
            "effects = []",
            "effects = ['unknown']",
            "typo = true",
        ] {
            assert!(toml::from_str::<Screensaver>(bad).is_err(), "{bad}");
        }
        let c: Screensaver = toml::from_str("effects = ['matrix', 'matrix']").unwrap();
        assert_eq!(c.effects, vec![Effect::Matrix]);
    }
}
