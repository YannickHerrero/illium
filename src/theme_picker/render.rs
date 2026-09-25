//! CPU raster cards for Slint's software renderer: centered crops, not shears.
//! Geometry and compositing follow the pinned Omarchy picker (MIT attribution in docs).
use super::{EXPANDED_HEIGHT, EXPANDED_WIDTH, SKEW, SLICE_HEIGHT, SLICE_WIDTH};
use illium_theme::{pack, preview::Entry};
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Two logical pixels reserve the outer half of the selected 3px stroke.
pub const PAD: f32 = 2.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub background: [u8; 3],
    pub foreground: [u8; 3],
    pub accent: [u8; 3],
}
impl Colors {
    pub fn from_theme(theme: &illium_theme::Theme) -> Self {
        let rgb = |s: &str| {
            let (r, g, b) = illium_theme::rgb(s).unwrap_or_default();
            [r, g, b]
        };
        Self {
            background: rgb(&theme.background),
            foreground: rgb(&theme.text),
            accent: rgb(&theme.accent),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub entry: Entry,
    pub dpi: u32,
    pub selected: bool,
    pub colors: Colors,
}
#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA, including transparent antialiased edges.
    pub pixels: Vec<u8>,
}
impl Frame {
    pub fn bytes(&self) -> usize {
        self.pixels.len()
    }
}

pub fn thumbnail(entry: &Entry) -> Result<image::RgbaImage, String> {
    // Same intermediate crop as omarchy-menu-images' vipsthumbnail. Keep the
    // decoded pixels lossless in memory rather than accumulating JPEG damage.
    let image = pack::decode(&entry.path)?;
    pack::cover(&image, 1536, 864)
}

pub fn card(thumbnail: &image::RgbaImage, key: &Key) -> Result<Frame, String> {
    if !(48..=768).contains(&key.dpi) {
        return Err("unsupported preview DPI".into());
    }
    let scale = key.dpi as f32 / 96.0;
    let (width, height) = if key.selected {
        (EXPANDED_WIDTH, EXPANDED_HEIGHT)
    } else {
        (SLICE_WIDTH, SLICE_HEIGHT)
    };
    let w = ((width + 2.0 * PAD) * scale).round() as u32;
    let h = ((height + 2.0 * PAD) * scale).round() as u32;
    let mut pixmap = Pixmap::new(w, h).ok_or("preview allocation failed")?;
    // Fit to the unpadded rectangular image bounds, then apply an oblique mask.
    let fitted = pack::cover(
        thumbnail,
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    )?;
    let offset = (PAD * scale).round() as u32;
    for y in 0..fitted.height() {
        for x in 0..fitted.width() {
            let rgba = fitted.get_pixel(x, y).0;
            let [r, g, b, a] = rgba;
            let alpha = a as f32 / 255.0;
            let dim = if key.selected { 0.0 } else { 0.42 };
            let out_alpha = dim + alpha * (1.0 - dim);
            let mut channels = [0u8; 4];
            for (i, value) in [r, g, b].into_iter().enumerate() {
                channels[i] = (key.colors.background[i] as f32 * dim
                    + value as f32 * alpha * (1.0 - dim))
                    .round() as u8;
            }
            channels[3] = (255.0 * out_alpha).round() as u8;
            let i = (((y + offset) * w + x + offset) * 4) as usize;
            pixmap.data_mut()[i..i + 4].copy_from_slice(&channels);
        }
    }
    let mut path = PathBuilder::new();
    path.move_to(PAD + SKEW, PAD);
    path.line_to(PAD + width, PAD);
    path.line_to(PAD + width - SKEW, PAD + height);
    path.line_to(PAD, PAD + height);
    path.close();
    let path = path.finish().ok_or("invalid preview path")?;
    let transform = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(w, h).ok_or("preview mask allocation failed")?;
    mask.fill_path(&path, FillRule::Winding, true, transform);
    pixmap.apply_mask(&mask);
    let mut paint = Paint::default();
    let [r, g, b] = if key.selected {
        key.colors.accent
    } else {
        key.colors.foreground
    };
    paint.set_color_rgba8(r, g, b, if key.selected { 255 } else { 71 });
    pixmap.stroke_path(
        &path,
        &paint,
        &Stroke {
            width: if key.selected { 3.0 } else { 1.0 },
            ..Stroke::default()
        },
        transform,
        None,
    );
    Ok(Frame {
        width: w,
        height: h,
        pixels: pixmap.take(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(selected: bool) -> Key {
        Key {
            entry: Entry {
                id: "test".into(),
                path: "unused".into(),
                size: 0,
                modified: 0,
            },
            dpi: 96,
            selected,
            colors: Colors {
                background: [0, 0, 0],
                foreground: [255, 255, 255],
                accent: [255, 0, 0],
            },
        }
    }
    fn pixel(frame: &Frame, x: u32, y: u32) -> &[u8] {
        &frame.pixels[((y * frame.width + x) * 4) as usize..][..4]
    }
    #[test]
    fn transparent_corners_borders_and_dimmed_center() {
        let image = image::RgbaImage::from_pixel(16, 9, image::Rgba([100, 200, 50, 255]));
        let f = card(&image, &key(true)).unwrap();
        assert_eq!((f.width, f.height), (772, 479));
        assert_eq!(pixel(&f, 2, 2), [0, 0, 0, 0]);
        assert_eq!(pixel(&f, 400, 240), [100, 200, 50, 255]);
        assert_eq!(pixel(&f, 400, 2), [255, 0, 0, 255]);
        let f = card(&image, &key(false)).unwrap();
        assert_eq!((f.width, f.height), (112, 436));
        assert_eq!(pixel(&f, 56, 218), [58, 116, 29, 255]);
        assert!(
            f.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] > 0 && p[3] < 255)
        );
    }
    #[test]
    fn physical_dimensions_track_dpi_and_bad_sizes_are_rejected() {
        let image = image::RgbaImage::new(2, 2);
        for dpi in [96, 120, 144, 192] {
            let mut k = key(true);
            k.dpi = dpi;
            let f = card(&image, &k).unwrap();
            assert_eq!(f.width, (772.0 * dpi as f32 / 96.0).round() as u32);
            assert!(
                f.pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[..3].iter().all(|c| *c <= p[3]))
            );
        }
        let mut k = key(true);
        k.dpi = u32::MAX;
        assert!(card(&image, &k).is_err());
    }
}
