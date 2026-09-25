//! Port of `terminaltexteffects.utils.graphics` (release 0.15.0).
use super::geometry::{Coord, find_normalized_distance_from_center};
use super::rng::Rng;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub [u8; 3]);

impl Color {
    /// Parses `rrggbb` (with or without `#`). Only used on literals.
    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        let v = u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("bad color literal {s}"));
        Self([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    }
    /// XTerm-256 palette index, as `hexterm.xterm_to_hex`.
    pub fn xterm(index: u8) -> Self {
        const SYSTEM: [u32; 16] = [
            0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0,
            0x808080, 0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff,
        ];
        let v = match index {
            0..=15 => SYSTEM[index as usize],
            16..=231 => {
                let i = index as u32 - 16;
                let level = |n: u32| if n == 0 { 0 } else { 55 + n * 40 };
                (level(i / 36) << 16) | (level(i / 6 % 6) << 8) | level(i % 6)
            }
            _ => {
                let g = 8 + (index as u32 - 232) * 10;
                (g << 16) | (g << 8) | g
            }
        };
        Self([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    }
    pub fn rgb(self) -> [u8; 3] {
        self.0
    }
}

pub fn colors(hex: &[&str]) -> Vec<Color> {
    hex.iter().map(|h| Color::hex(h)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ColorPair {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

impl ColorPair {
    pub fn fg(fg: Color) -> Self {
        Self {
            fg: Some(fg),
            bg: None,
        }
    }
    pub fn bg(bg: Color) -> Self {
        Self {
            fg: None,
            bg: Some(bg),
        }
    }
    pub fn new(fg: Option<Color>, bg: Option<Color>) -> Self {
        Self { fg, bg }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Vertical,
    Horizontal,
    Radial,
    Diagonal,
}

#[derive(Clone, Debug)]
pub struct Gradient {
    pub spectrum: Vec<Color>,
}

impl Gradient {
    pub fn new(stops: &[Color], steps: &[usize]) -> Self {
        Self::build(stops, steps, false)
    }
    pub fn looped(stops: &[Color], steps: &[usize]) -> Self {
        Self::build(stops, steps, true)
    }
    fn build(stops: &[Color], steps: &[usize], looped: bool) -> Self {
        assert!(!stops.is_empty() && !steps.is_empty());
        let mut spectrum = Vec::new();
        if stops.len() == 1 {
            spectrum.extend(std::iter::repeat_n(stops[0], steps[0]));
            return Self { spectrum };
        }
        let mut stops = stops.to_vec();
        if looped {
            stops.push(stops[0]);
        }
        let pairs = stops.len() - 1;
        let mut steps: Vec<usize> = steps.iter().copied().take(pairs).collect();
        while steps.len() < pairs {
            steps.push(*steps.last().unwrap());
        }
        for (pair, &count) in stops.windows(2).zip(&steps) {
            let (start, end) = (pair[0].0, pair[1].0);
            let delta = |i: usize| (end[i] as i32 - start[i] as i32).div_euclid(count as i32);
            let deltas = [delta(0), delta(1), delta(2)];
            let range_start = usize::from(!spectrum.is_empty());
            for i in range_start..count {
                let channel =
                    |c: usize| (start[c] as i32 + deltas[c] * i as i32).clamp(0, 255) as u8;
                spectrum.push(Color([channel(0), channel(1), channel(2)]));
            }
            spectrum.push(pair[1]);
        }
        Self { spectrum }
    }
    pub fn len(&self) -> usize {
        self.spectrum.len()
    }
    pub fn is_empty(&self) -> bool {
        self.spectrum.is_empty()
    }
    pub fn get_color_at_fraction(&self, fraction: f64) -> Color {
        let n = self.spectrum.len();
        for i in 1..=n {
            if fraction <= i as f64 / n as f64 {
                return self.spectrum[i - 1];
            }
        }
        self.spectrum[n - 1]
    }
    pub fn build_coordinate_color_mapping(
        &self,
        min_row: i32,
        max_row: i32,
        min_column: i32,
        max_column: i32,
        direction: Direction,
    ) -> HashMap<Coord, Color> {
        let row_offset = min_row - 1;
        let column_offset = min_column - 1;
        let mut map = HashMap::new();
        for row in min_row..=max_row {
            for column in min_column..=max_column {
                let fraction = match direction {
                    Direction::Vertical => {
                        (row - row_offset) as f64 / (max_row - row_offset) as f64
                    }
                    Direction::Horizontal => {
                        (column - column_offset) as f64 / (max_column - column_offset) as f64
                    }
                    Direction::Radial => find_normalized_distance_from_center(
                        min_row,
                        max_row,
                        min_column,
                        max_column,
                        Coord::new(column, row),
                    ),
                    Direction::Diagonal => {
                        ((row - row_offset) * 2 + (column - column_offset)) as f64
                            / ((max_row - row_offset) * 2 + (max_column - column_offset)) as f64
                    }
                };
                map.insert(
                    Coord::new(column, row),
                    self.get_color_at_fraction(fraction),
                );
            }
        }
        map
    }
}

impl std::ops::Index<usize> for Gradient {
    type Output = Color;
    fn index(&self, i: usize) -> &Color {
        &self.spectrum[i]
    }
}

pub fn random_color(rng: &mut Rng) -> Color {
    let v = rng.randint(0, 0xffffff) as u32;
    Color([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub fn shift_color_towards(color: Color, target: Color, factor: f64) -> Color {
    let channel = |i: usize| {
        let a = color.0[i] as f64 / 255.0;
        let b = target.0[i] as f64 / 255.0;
        ((a + (b - a) * factor) * 255.0) as u8
    };
    Color([channel(0), channel(1), channel(2)])
}

/// `Animation.adjust_color_brightness` (HSL lightness scaling).
pub fn adjust_color_brightness(color: Color, brightness: f64) -> Color {
    fn hue_to_rgb(lightness_scaled: f64, intensity: f64, mut hue: f64) -> f64 {
        if hue < 0.0 {
            hue += 1.0;
        }
        if hue > 1.0 {
            hue -= 1.0;
        }
        if hue < 1.0 / 6.0 {
            return lightness_scaled + (intensity - lightness_scaled) * 6.0 * hue;
        }
        if hue < 0.5 {
            return intensity;
        }
        if hue < 2.0 / 3.0 {
            return lightness_scaled + (intensity - lightness_scaled) * (2.0 / 3.0 - hue) * 6.0;
        }
        lightness_scaled
    }
    let [r, g, b] = color.0.map(|c| c as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let mut lightness = (max + min) / 2.0;
    let (hue, saturation) = if max == min {
        (0.0, 0.0)
    } else {
        let diff = max - min;
        let saturation = if lightness > 0.5 {
            diff / (2.0 - max - min)
        } else {
            diff / (max + min)
        };
        let hue = if max == r {
            (g - b) / diff + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / diff + 2.0
        } else {
            (r - g) / diff + 4.0
        };
        (hue / 6.0, saturation)
    };
    lightness = (lightness * brightness).clamp(0.0, 1.0);
    let (r, g, b) = if saturation == 0.0 {
        (lightness, lightness, lightness)
    } else {
        let intensity = if lightness < 0.5 {
            lightness * (1.0 + saturation)
        } else {
            lightness + saturation - lightness * saturation
        };
        let scaled = 2.0 * lightness - intensity;
        (
            hue_to_rgb(scaled, intensity, hue + 1.0 / 3.0),
            hue_to_rgb(scaled, intensity, hue),
            hue_to_rgb(scaled, intensity, hue - 1.0 / 3.0),
        )
    };
    Color([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_matches_python_spectrum() {
        let g = Gradient::new(&[Color::hex("ffffff"), Color::hex("000000")], &[4]);
        let hex: Vec<_> = g
            .spectrum
            .iter()
            .map(|c| format!("{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2]))
            .collect();
        // Python floors -255 // 4 to -64.
        assert_eq!(hex, ["ffffff", "bfbfbf", "7f7f7f", "3f3f3f", "000000"]);
        let one = Gradient::new(&[Color::hex("eda000")], &[12]);
        assert_eq!(one.len(), 12);
        let looped = Gradient::looped(&colors(&["ff0000", "00ff00"]), &[2]);
        assert_eq!(looped.len(), 5);
        assert_eq!(g.get_color_at_fraction(0.0), Color::hex("ffffff"));
        assert_eq!(g.get_color_at_fraction(1.0), Color::hex("000000"));
    }
    #[test]
    fn brightness_and_shift() {
        assert_eq!(
            adjust_color_brightness(Color::hex("808080"), 0.5),
            Color::hex("404040")
        );
        assert_eq!(
            shift_color_towards(Color::hex("000000"), Color::hex("ffffff"), 0.5),
            Color::hex("7f7f7f")
        );
        assert_eq!(Color::xterm(196), Color::hex("ff0000"));
    }
}
