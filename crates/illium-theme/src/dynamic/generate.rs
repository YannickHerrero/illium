//! Deterministic alpha-weighted quantization, followed by gamut-mapped OKLCH.
use super::Palettes;
use crate::Theme;

fn linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn lab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(linear);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ]
}
fn color(l: f64, mut c: f64, hue: f64) -> String {
    let h = hue.to_radians();
    // Reduce chroma rather than clipping RGB channels (which changes the hue).
    loop {
        let (a, b) = (c * h.cos(), c * h.sin());
        let x = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let y = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let z = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
        let rgb = [
            4.0767416621 * x - 3.3077115913 * y + 0.2309699292 * z,
            -1.2684380046 * x + 2.6097574011 * y - 0.3413193965 * z,
            -0.0041960863 * x - 0.7034186147 * y + 1.707614701 * z,
        ];
        if rgb.iter().all(|v| (0.0..=1.0).contains(v)) || c < 0.0001 {
            let [r, g, b] = rgb.map(|v| {
                let v = v.clamp(0.0, 1.0);
                let s = if v <= 0.0031308 {
                    12.92 * v
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                };
                (s * 255.0).round() as u8
            });
            return format!("#{r:02x}{g:02x}{b:02x}");
        }
        c *= 0.9;
    }
}
fn luminance(s: &str) -> f64 {
    let (r, g, b) = crate::rgb(s).unwrap();
    0.2126 * linear(r as f64 / 255.0)
        + 0.7152 * linear(g as f64 / 255.0)
        + 0.0722 * linear(b as f64 / 255.0)
}
fn contrast(a: &str, b: &str) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(mut l: f64, c: f64, h: f64, dark: bool, backgrounds: &[&str]) -> String {
    for _ in 0..101 {
        let rgb = color(l, c, h);
        if backgrounds.iter().all(|bg| contrast(&rgb, bg) >= 4.5) {
            return rgb;
        }
        l = (l + if dark { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
    }
    color(if dark { 1.0 } else { 0.0 }, 0.0, h)
}
fn palette(h: f64, chroma: f64, dark: bool) -> Theme {
    let mut t = Theme::default_theme();
    t.name = format!("Dynamic {}", if dark { "Dark" } else { "Light" });
    t.mode = Some(if dark { "dark" } else { "light" }.into());
    let c = chroma.clamp(0.015, 0.035);
    t.background = color(if dark { 0.18 } else { 0.97 }, c * 0.5, h);
    t.surface = color(if dark { 0.24 } else { 0.93 }, c * 0.7, h);
    t.overlay = color(if dark { 0.32 } else { 0.86 }, c, h);
    let backgrounds = [&*t.background, &*t.surface, &*t.overlay];
    t.text = readable(
        if dark { 0.94 } else { 0.22 },
        c * 0.4,
        h,
        dark,
        &backgrounds,
    );
    t.subtext = readable(
        if dark { 0.73 } else { 0.43 },
        c * 0.5,
        h,
        dark,
        &backgrounds,
    );
    let l = if dark { 0.78 } else { 0.43 };
    t.accent = readable(l, chroma.clamp(0.10, 0.18), h, dark, &backgrounds);
    // Gently harmonize ANSI hues with the image without turning errors green
    // or losing the blue/cyan/magenta distinctions applications depend on.
    let harmonize =
        |base: f64| base + ((h - base + 180.0).rem_euclid(360.0) - 180.0).clamp(-12.0, 12.0);
    let terminal_chroma = chroma.clamp(0.10, 0.16);
    let semantic = |h| readable(l, terminal_chroma, harmonize(h), dark, &backgrounds);
    t.red = semantic(25.0);
    t.green = semantic(145.0);
    t.yellow = semantic(85.0);
    let blue = semantic(255.0);
    let magenta = semantic(315.0);
    let cyan = semantic(195.0);
    t.ansi = Some(vec![
        t.overlay.clone(),
        t.red.clone(),
        t.green.clone(),
        t.yellow.clone(),
        blue,
        magenta,
        cyan,
        t.text.clone(),
    ]);
    // Bright colors keep ANSI meanings in light mode too; "bright" need not mean
    // less readable on a light background.
    let bright = |h| {
        readable(
            if dark { 0.87 } else { 0.36 },
            terminal_chroma + 0.02,
            harmonize(h),
            dark,
            &backgrounds,
        )
    };
    t.brights = Some(vec![
        t.subtext.clone(),
        bright(25.0),
        bright(145.0),
        bright(85.0),
        bright(255.0),
        bright(315.0),
        bright(195.0),
        t.text.clone(),
    ]);
    t
}
/// At most 128×128 samples, independent of monitor crops. Empty/transparent or
/// achromatic images use a restrained blue accent rather than unstable hue noise.
pub fn generate(image: &image::RgbaImage) -> Palettes {
    let mut bins = vec![(0.0f64, [0.0f64; 3]); 4096];
    let (w, h) = image.dimensions();
    for y in 0..h.min(128) {
        for x in 0..w.min(128) {
            let p = image.get_pixel(x * w / w.min(128), y * h / h.min(128)).0;
            let weight = p[3] as f64 / 255.0;
            let index =
                ((p[0] as usize >> 4) << 8) | ((p[1] as usize >> 4) << 4) | (p[2] as usize >> 4);
            bins[index].0 += weight;
            for (sum, v) in bins[index].1.iter_mut().zip(p) {
                *sum += v as f64 / 255.0 * weight;
            }
        }
    }
    let mut best = (0.0, 255.0, 0.10);
    for (weight, sum) in bins {
        if weight == 0.0 {
            continue;
        }
        let [l, a, b] = lab(sum.map(|v| v / weight));
        let c = a.hypot(b);
        if c < 0.025 || !(0.15..0.95).contains(&l) {
            continue;
        }
        let score = weight.sqrt() * c;
        if score > best.0 {
            best = (score, b.atan2(a).to_degrees(), c);
        }
    }
    Palettes {
        dark: palette(best.1, best.2, true),
        light: palette(best.1, best.2, false),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_readable_complete_palettes_even_for_degenerate_images() {
        for rgba in [
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [128, 128, 128, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 0, 255, 0],
        ] {
            let image = image::RgbaImage::from_pixel(20, 30, image::Rgba(rgba));
            let pair = generate(&image);
            assert_eq!(pair, generate(&image));
            pair.validate().unwrap();
            for t in [&pair.dark, &pair.light] {
                for fg in [&t.text, &t.subtext, &t.accent, &t.red, &t.green, &t.yellow] {
                    for bg in [&t.background, &t.surface, &t.overlay] {
                        assert!(contrast(fg, bg) >= 4.5, "{fg} on {bg}");
                    }
                }
                assert_eq!(t.ansi.as_ref().unwrap().len(), 8);
                assert_eq!(t.brights.as_ref().unwrap().len(), 8);
            }
        }
        generate(&image::RgbaImage::new(0, 0)).validate().unwrap();
    }
    #[test]
    fn source_colors_change_accent_and_transparent_pixels_do_not() {
        let red = image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 40, 0, 255]));
        let blue = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 40, 255, 255]));
        assert_ne!(generate(&red).dark.accent, generate(&blue).dark.accent);
        assert_ne!(generate(&red).dark.red, generate(&blue).dark.red);
        assert_ne!(generate(&red).light.brights, generate(&blue).light.brights);
        let invisible = image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 40, 0, 0]));
        assert_eq!(
            generate(&invisible),
            generate(&image::RgbaImage::new(32, 32))
        );
    }
}
