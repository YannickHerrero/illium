//! One module per TTE effect, ported from release 0.15.0 with its defaults
//! (`tte --random-effect` never passes effect options).
use crate::engine::{Effect as Run, Terminal};
use winarchy_config::screensaver::Effect;

mod burn;
mod beams;
mod colorshift;
mod binarypath;
mod bouncyballs;
mod crumble;
mod blackhole;
mod bubbles;
mod decrypt;
mod laseretch;
mod pour;
mod print;
mod smoke;
mod rain;
mod synthgrid;
mod thunderstorm;
mod vhstape;
mod highlight;
mod matrix;
mod sweep;
mod wipe;
mod errorcorrect;
mod expand;
mod middleout;
mod orbittingvolley;
mod fireworks;
mod rings;
mod spotlights;
mod randomsequence;
mod scattered;

/// The Winarchy logo in Omarchy's `logo.txt` style ("ARCHY" is theirs).
pub const LOGO: &str = include_str!("../logo.txt");

pub fn build(effect: Effect, t: &mut Terminal) -> Box<dyn Run> {
    match effect {
        Effect::Burn => Box::new(burn::Burn::new(t)),
        Effect::Beams => Box::new(beams::Beams::new(t)),
        Effect::ColorShift => Box::new(colorshift::ColorShift::new(t)),
        Effect::Blackhole => Box::new(blackhole::Blackhole::new(t)),
        Effect::Bubbles => Box::new(bubbles::Bubbles::new(t)),
        Effect::Decrypt => Box::new(decrypt::Decrypt::new(t)),
        Effect::LaserEtch => Box::new(laseretch::LaserEtch::new(t)),
        Effect::Pour => Box::new(pour::Pour::new(t)),
        Effect::Print => Box::new(print::Print::new(t)),
        Effect::Smoke => Box::new(smoke::Smoke::new(t)),
        Effect::Rain => Box::new(rain::Rain::new(t)),
        Effect::SynthGrid => Box::new(synthgrid::SynthGrid::new(t)),
        Effect::VhsTape => Box::new(vhstape::VhsTape::new(t)),
        Effect::Thunderstorm => Box::new(thunderstorm::Thunderstorm::new(t)),
        Effect::Matrix => Box::new(matrix::Matrix::new(t)),
        Effect::Highlight => Box::new(highlight::Highlight::new(t)),
        Effect::Sweep => Box::new(sweep::Sweep::new(t)),
        Effect::Wipe => Box::new(wipe::Wipe::new(t)),
        Effect::Expand => Box::new(expand::Expand::new(t)),
        Effect::MiddleOut => Box::new(middleout::MiddleOut::new(t)),
        Effect::BouncyBalls => Box::new(bouncyballs::BouncyBalls::new(t)),
        Effect::BinaryPath => Box::new(binarypath::BinaryPath::new(t)),
        Effect::Crumble => Box::new(crumble::Crumble::new(t)),
        Effect::ErrorCorrect => Box::new(errorcorrect::ErrorCorrect::new(t)),
        Effect::OrbittingVolley => Box::new(orbittingvolley::OrbittingVolley::new(t)),
        Effect::Fireworks => Box::new(fireworks::Fireworks::new(t)),
        Effect::Rings => Box::new(rings::Rings::new(t)),
        Effect::Spotlights => Box::new(spotlights::Spotlights::new(t)),
        Effect::RandomSequence => Box::new(randomsequence::RandomSequence::new(t)),
        Effect::Scattered => Box::new(scattered::Scattered::new(t)),
        other => unimplemented!("{other:?} is not ported yet"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::{CharacterSort, Select};

    /// Runs an effect to completion; it must end on the logo, fully visible.
    /// Returns the number of frames.
    pub fn finishes_on_logo(effect: Effect, max_frames: usize) -> usize {
        let mut t = Terminal::new(LOGO, 140, 40, 7);
        let mut run = build(effect, &mut t);
        let mut frames = 0;
        while run.next(&mut t) {
            frames += 1;
            assert!(
                frames < max_frames,
                "{effect:?} did not finish in {max_frames} frames"
            );
        }
        let cells = t.render();
        for id in t.get_characters(Select::INPUT, CharacterSort::TopToBottomLeftToRight) {
            let c = &t.chars[id];
            let at = c.input_coord;
            let cell = cells[((t.canvas.top - at.row) * t.canvas.right + at.column - 1) as usize];
            assert_eq!(cell.symbol, c.input_symbol, "{effect:?} left {at:?} wrong");
        }
        frames
    }

    #[test]
    fn beams_finishes() {
        finishes_on_logo(Effect::Beams, 20_000);
    }

    #[test]
    fn colorshift_finishes() {
        finishes_on_logo(Effect::ColorShift, 5_000);
    }

    #[test]
    fn blackhole_finishes() {
        finishes_on_logo(Effect::Blackhole, 20_000);
    }

    #[test]
    fn bubbles_finishes() {
        finishes_on_logo(Effect::Bubbles, 20_000);
    }

    #[test]
    fn decrypt_finishes() {
        finishes_on_logo(Effect::Decrypt, 20_000);
    }

    #[test]
    fn burn_finishes() {
        finishes_on_logo(Effect::Burn, 20_000);
    }

    #[test]
    fn laseretch_finishes() {
        finishes_on_logo(Effect::LaserEtch, 20_000);
    }

    #[test]
    fn smoke_finishes() {
        finishes_on_logo(Effect::Smoke, 20_000);
    }

    #[test]
    fn print_finishes() {
        finishes_on_logo(Effect::Print, 20_000);
    }

    #[test]
    fn pour_finishes() {
        finishes_on_logo(Effect::Pour, 20_000);
    }

    #[test]
    fn rain_finishes() {
        finishes_on_logo(Effect::Rain, 20_000);
    }

    #[test]
    fn synthgrid_finishes() {
        finishes_on_logo(Effect::SynthGrid, 20_000);
    }

    #[test]
    fn vhstape_finishes() {
        finishes_on_logo(Effect::VhsTape, 20_000);
    }

    #[test]
    fn thunderstorm_finishes() {
        finishes_on_logo(Effect::Thunderstorm, 20_000);
    }

    #[test]
    fn matrix_finishes() {
        finishes_on_logo(Effect::Matrix, 20_000);
    }

    #[test]
    fn highlight_finishes() {
        finishes_on_logo(Effect::Highlight, 5_000);
    }

    #[test]
    fn sweep_finishes() {
        finishes_on_logo(Effect::Sweep, 5_000);
    }

    #[test]
    fn wipe_finishes() {
        finishes_on_logo(Effect::Wipe, 5_000);
    }

    #[test]
    fn expand_finishes() {
        finishes_on_logo(Effect::Expand, 20_000);
    }

    #[test]
    fn middleout_finishes() {
        finishes_on_logo(Effect::MiddleOut, 20_000);
    }

    #[test]
    fn bouncyballs_finishes() {
        finishes_on_logo(Effect::BouncyBalls, 20_000);
    }

    #[test]
    fn binarypath_finishes() {
        finishes_on_logo(Effect::BinaryPath, 20_000);
    }

    #[test]
    fn crumble_finishes() {
        finishes_on_logo(Effect::Crumble, 20_000);
    }

    #[test]
    fn errorcorrect_finishes() {
        finishes_on_logo(Effect::ErrorCorrect, 20_000);
    }

    #[test]
    fn orbittingvolley_finishes() {
        finishes_on_logo(Effect::OrbittingVolley, 20_000);
    }

    #[test]
    fn fireworks_finishes() {
        finishes_on_logo(Effect::Fireworks, 20_000);
    }

    #[test]
    fn rings_finishes() {
        finishes_on_logo(Effect::Rings, 20_000);
    }

    #[test]
    fn spotlights_finishes() {
        finishes_on_logo(Effect::Spotlights, 20_000);
    }

    #[test]
    fn randomsequence_finishes() {
        finishes_on_logo(Effect::RandomSequence, 20_000);
    }

    #[test]
    fn scattered_finishes() {
        finishes_on_logo(Effect::Scattered, 20_000);
    }
}
