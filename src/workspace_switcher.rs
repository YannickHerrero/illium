//! Workspace carousel geometry and animation, independent of Windows/Slint.
use crate::layout::Rect;

pub const DURATION: f32 = 0.22;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl BoxRect {
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let w = (self.x + self.w).min(other.x + other.w) - x;
        let h = (self.y + self.h).min(other.y + other.h) - y;
        (w > 0.0 && h > 0.0).then_some(Self { x, y, w, h })
    }
    /// The non-overlapping pieces left after an opaque window covers this one.
    pub fn subtract(self, cover: Self) -> Vec<Self> {
        let Some(hit) = self.intersection(cover) else {
            return vec![self];
        };
        [
            Self {
                x: self.x,
                y: self.y,
                w: self.w,
                h: hit.y - self.y,
            },
            Self {
                x: self.x,
                y: hit.y + hit.h,
                w: self.w,
                h: self.y + self.h - hit.y - hit.h,
            },
            Self {
                x: self.x,
                y: hit.y,
                w: hit.x - self.x,
                h: hit.h,
            },
            Self {
                x: hit.x + hit.w,
                y: hit.y,
                w: self.x + self.w - hit.x - hit.w,
                h: hit.h,
            },
        ]
        .into_iter()
        .filter(|r| r.w > 0.0 && r.h > 0.0)
        .collect()
    }
    /// Fit a monitor into a card without changing its aspect ratio.
    pub fn fit(self, monitor: Rect) -> Self {
        let scale = (self.w / monitor.w.max(1) as f32).min(self.h / monitor.h.max(1) as f32);
        let w = monitor.w as f32 * scale;
        let h = monitor.h as f32 * scale;
        Self {
            x: self.x + (self.w - w) / 2.0,
            y: self.y + (self.h - h) / 2.0,
            w,
            h,
        }
    }
}

#[derive(Debug)]
pub struct Model {
    pub selected: u8,
    pub position: f32,
    start: f32,
    elapsed: f32,
}
impl Model {
    pub fn new(active: u8) -> Self {
        let selected = active.clamp(1, 9);
        Self {
            selected,
            position: selected as f32,
            start: selected as f32,
            elapsed: DURATION,
        }
    }
    pub fn select(&mut self, selected: u8) {
        let selected = selected.clamp(1, 9);
        if self.selected != selected {
            self.start = self.position;
            self.selected = selected;
            self.elapsed = 0.0;
        }
    }
    pub fn step(&mut self, delta: i8) {
        self.select((self.selected as i16 + delta as i16).clamp(1, 9) as u8);
    }
    pub fn advance(&mut self, seconds: f32) -> bool {
        if self.elapsed >= DURATION {
            return false;
        }
        self.elapsed = (self.elapsed + seconds.max(0.0)).min(DURATION);
        let t = 1.0 - (1.0 - self.elapsed / DURATION).powi(3);
        self.position = self.start + (self.selected as f32 - self.start) * t;
        true
    }
    pub fn card(&self, workspace: u8, width: f32, height: f32) -> (BoxRect, f32) {
        let base_w = (width * 0.26).min(520.0);
        let base_h = (base_w * 0.625).min(height * 0.46);
        let distance = workspace as f32 - self.position;
        let prominence = 1.0 - distance.abs().min(1.0);
        let scale = 1.0 + 0.045 * prominence;
        let w = base_w * scale;
        let h = base_h * scale;
        (
            BoxRect {
                x: width / 2.0 + distance * (base_w * 1.09) - w / 2.0,
                y: height * 0.47 - h / 2.0,
                w,
                h,
            },
            0.72 + 0.28 * prominence,
        )
    }
}

/// Map a visible window frame into a miniature desktop, clipping both the
/// destination and source proportionally (DWM does not inherit Slint clipping).
pub fn thumbnail(
    frame: Rect,
    source: Rect,
    monitor: Rect,
    desktop: BoxRect,
    viewport: BoxRect,
) -> Option<(BoxRect, Rect)> {
    if frame.w <= 0 || frame.h <= 0 || source.w <= 0 || source.h <= 0 {
        return None;
    }
    let sx = desktop.w / monitor.w.max(1) as f32;
    let sy = desktop.h / monitor.h.max(1) as f32;
    let dest = BoxRect {
        x: desktop.x + (frame.x - monitor.x) as f32 * sx,
        y: desktop.y + (frame.y - monitor.y) as f32 * sy,
        w: frame.w as f32 * sx,
        h: frame.h as f32 * sy,
    };
    let clipped = dest.intersection(desktop)?.intersection(viewport)?;
    let left = ((clipped.x - dest.x) / dest.w * source.w as f32).round() as i32;
    let top = ((clipped.y - dest.y) / dest.h * source.h as f32).round() as i32;
    let right = ((clipped.x + clipped.w - dest.x) / dest.w * source.w as f32).round() as i32;
    let bottom = ((clipped.y + clipped.h - dest.y) / dest.h * source.h as f32).round() as i32;
    let region = Rect {
        x: source.x + left,
        y: source.y + top,
        w: right - left,
        h: bottom - top,
    };
    (region.w > 0 && region.h > 0).then_some((clipped, region))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_is_bounded_and_retargeting_does_not_jump() {
        let mut m = Model::new(1);
        m.step(-1);
        assert_eq!(m.selected, 1);
        m.select(5);
        m.advance(0.07);
        let position = m.position;
        m.step(1);
        assert_eq!(m.position, position);
        m.advance(DURATION);
        assert_eq!(m.position, 6.0);
        assert!(!m.advance(0.01));
        m.select(9);
        m.step(1);
        assert_eq!(m.selected, 9);
    }
    #[test]
    fn selected_card_is_centered_and_neighbours_do_not_overlap() {
        for width in [640.0, 1920.0, 3840.0] {
            let m = Model::new(5);
            let (a, _) = m.card(5, width, 1080.0);
            let (b, _) = m.card(6, width, 1080.0);
            assert!((a.x + a.w / 2.0 - width / 2.0).abs() < 0.01);
            assert!(a.x + a.w < b.x);
        }
    }
    #[test]
    fn clipping_preserves_source_mapping_with_negative_monitor_coordinates() {
        let monitor = Rect {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let desktop = BoxRect {
            x: 0.0,
            y: 0.0,
            w: 960.0,
            h: 540.0,
        };
        let frame = Rect {
            x: -2020,
            y: 100,
            w: 400,
            h: 200,
        };
        let source = Rect {
            x: 8,
            y: 8,
            w: 400,
            h: 200,
        };
        let (dest, region) = thumbnail(frame, source, monitor, desktop, desktop).unwrap();
        assert_eq!(
            dest,
            BoxRect {
                x: 0.0,
                y: 50.0,
                w: 150.0,
                h: 100.0
            }
        );
        assert_eq!(
            region,
            Rect {
                x: 108,
                y: 8,
                w: 300,
                h: 200
            }
        );
        assert!(
            thumbnail(
                frame,
                source,
                monitor,
                desktop,
                BoxRect {
                    x: 500.0,
                    ..desktop
                }
            )
            .is_none()
        );
    }
    #[test]
    fn occlusion_pieces_do_not_overlap_and_preserve_uncovered_area() {
        let rect = BoxRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 80.0,
        };
        let cover = BoxRect {
            x: 20.0,
            y: 10.0,
            w: 40.0,
            h: 30.0,
        };
        let pieces = rect.subtract(cover);
        assert_eq!(pieces.len(), 4);
        assert_eq!(pieces.iter().map(|r| r.w * r.h).sum::<f32>(), 6800.0);
        for (i, piece) in pieces.iter().enumerate() {
            assert!(piece.intersection(cover).is_none());
            assert!(
                pieces[i + 1..]
                    .iter()
                    .all(|other| piece.intersection(*other).is_none())
            );
        }
        assert!(rect.subtract(rect).is_empty());
        assert_eq!(rect.subtract(BoxRect { x: 200.0, ..cover }), vec![rect]);
    }
    #[test]
    fn portrait_monitors_are_letterboxed() {
        let card = BoxRect {
            x: 10.0,
            y: 20.0,
            w: 400.0,
            h: 250.0,
        };
        let fit = card.fit(Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 2000,
        });
        assert_eq!(
            fit,
            BoxRect {
                x: 147.5,
                y: 20.0,
                w: 125.0,
                h: 250.0
            }
        );
    }
}
