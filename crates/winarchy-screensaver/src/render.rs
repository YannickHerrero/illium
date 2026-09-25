//! Terminal-like rasterizer: the canvas is drawn cell by cell into an RGB
//! buffer, the way Omarchy's full-screen terminal shows `tte` output.
//! Block elements are filled rectangles (as terminals draw them), so logo
//! strokes never show seams between cells.
use crate::engine::{Cell, Color};
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;

static FONT: &[u8] = include_bytes!("../../../ui/fonts/JetBrainsMono-Regular.ttf");

/// Omarchy launches the screensaver terminal with an 18 pt font.
pub const FONT_POINTS: f32 = 18.0;
/// Alacritty's default foreground, for cells an effect leaves uncolored.
pub const DEFAULT_FG: Color = Color([0xd8, 0xd8, 0xd8]);

pub struct Renderer {
    font: Font,
    px: f32,
    pub cell_width: usize,
    pub cell_height: usize,
    baseline: i32,
    pub columns: usize,
    pub rows: usize,
    pub width: usize,
    pub height: usize,
    offset_x: usize,
    offset_y: usize,
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
    pixels: Vec<u8>,
    previous: Vec<Cell>,
}

impl Renderer {
    /// `width`/`height` in physical pixels; `scale` is the monitor DPI scale.
    pub fn new(width: usize, height: usize, scale: f32) -> Self {
        let font = Font::from_bytes(FONT, FontSettings::default()).expect("bundled font");
        let px = (FONT_POINTS * 96.0 / 72.0 * scale).max(4.0);
        let line = font
            .horizontal_line_metrics(px)
            .expect("horizontal metrics");
        let cell_width = (font.metrics('M', px).advance_width.floor() as usize).max(1);
        let cell_height = ((line.ascent - line.descent + line.line_gap).floor() as usize).max(1);
        let baseline = cell_height as i32 + line.descent.round() as i32;
        let columns = (width / cell_width).max(1);
        let rows = (height / cell_height).max(1);
        Self {
            font,
            px,
            cell_width,
            cell_height,
            baseline,
            columns,
            rows,
            width,
            height,
            offset_x: (width.saturating_sub(columns * cell_width)) / 2,
            offset_y: (height.saturating_sub(rows * cell_height)) / 2,
            glyphs: HashMap::new(),
            pixels: vec![0; width * height * 3],
            previous: vec![],
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Redraws the cells that changed since the last frame; returns whether
    /// any pixel changed. `cells` is `rows * columns`, top row first.
    pub fn draw(&mut self, cells: &[Cell]) -> bool {
        assert_eq!(cells.len(), self.rows * self.columns);
        let full = self.previous.len() != cells.len();
        let mut changed = false;
        for (i, cell) in cells.iter().enumerate() {
            if !full && self.previous[i] == *cell {
                continue;
            }
            changed = true;
            self.draw_cell(i % self.columns, i / self.columns, cell);
        }
        self.previous = cells.to_vec();
        changed
    }

    fn fill(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, color: [u8; 3], alpha: f32) {
        for y in y0..y1.min(self.height) {
            let row = &mut self.pixels[y * self.width * 3..(y + 1) * self.width * 3];
            for x in x0..x1.min(self.width) {
                let p = &mut row[x * 3..x * 3 + 3];
                for c in 0..3 {
                    p[c] = (p[c] as f32 + (color[c] as f32 - p[c] as f32) * alpha).round() as u8;
                }
            }
        }
    }

    fn draw_cell(&mut self, column: usize, row: usize, cell: &Cell) {
        let (w, h) = (self.cell_width, self.cell_height);
        let x = self.offset_x + column * w;
        let y = self.offset_y + row * h;
        let bg = cell.colors.bg.map_or([0, 0, 0], |c| c.0);
        self.fill(x, y, x + w, y + h, bg, 1.0);
        if cell.symbol == ' ' {
            return;
        }
        let fg = cell.colors.fg.unwrap_or(DEFAULT_FG).0;
        if let Some(parts) = block(cell.symbol) {
            for (fx0, fy0, fx1, fy1, alpha) in parts {
                let px = |f: f32| x + (f * w as f32).round() as usize;
                let py = |f: f32| y + (f * h as f32).round() as usize;
                self.fill(px(fx0), py(fy0), px(fx1), py(fy1), fg, alpha);
            }
            return;
        }
        let px = self.px;
        let font = &self.font;
        let (metrics, bitmap) = self
            .glyphs
            .entry(cell.symbol)
            .or_insert_with(|| font.rasterize(cell.symbol, px));
        let (metrics, bitmap) = (*metrics, std::mem::take(bitmap));
        let gx = x as i32 + metrics.xmin;
        let gy = y as i32 + self.baseline - metrics.height as i32 - metrics.ymin;
        for by in 0..metrics.height {
            let py = gy + by as i32;
            if py < y as i32 || py >= (y + h) as i32 || py as usize >= self.height {
                continue;
            }
            for bx in 0..metrics.width {
                let pxl = gx + bx as i32;
                if pxl < x as i32 || pxl >= (x + w) as i32 || pxl as usize >= self.width {
                    continue;
                }
                let coverage = bitmap[by * metrics.width + bx];
                if coverage > 0 {
                    let (px, py) = (pxl as usize, py as usize);
                    self.fill(px, py, px + 1, py + 1, fg, coverage as f32 / 255.0);
                }
            }
        }
        self.glyphs.insert(cell.symbol, (metrics, bitmap));
    }
}

type Rect = (f32, f32, f32, f32, f32);

/// U+2580..U+259F as fractional rectangles `(x0, y0, x1, y1, alpha)` of the cell.
fn block(symbol: char) -> Option<Vec<Rect>> {
    let c = symbol as u32;
    let eighth = |n: u32| n as f32 / 8.0;
    let ul = (0.0, 0.0, 0.5, 0.5, 1.0);
    let ur = (0.5, 0.0, 1.0, 0.5, 1.0);
    let ll = (0.0, 0.5, 0.5, 1.0, 1.0);
    let lr = (0.5, 0.5, 1.0, 1.0, 1.0);
    Some(match c {
        0x2580 => vec![(0.0, 0.0, 1.0, 0.5, 1.0)],
        0x2581..=0x2587 => vec![(0.0, 1.0 - eighth(c - 0x2580), 1.0, 1.0, 1.0)],
        0x2588 => vec![(0.0, 0.0, 1.0, 1.0, 1.0)],
        0x2589..=0x258f => vec![(0.0, 0.0, eighth(0x2590 - c), 1.0, 1.0)],
        0x2590 => vec![(0.5, 0.0, 1.0, 1.0, 1.0)],
        0x2591 => vec![(0.0, 0.0, 1.0, 1.0, 0.25)],
        0x2592 => vec![(0.0, 0.0, 1.0, 1.0, 0.5)],
        0x2593 => vec![(0.0, 0.0, 1.0, 1.0, 0.75)],
        0x2594 => vec![(0.0, 0.0, 1.0, eighth(1), 1.0)],
        0x2595 => vec![(eighth(7), 0.0, 1.0, 1.0, 1.0)],
        0x2596 => vec![ll],
        0x2597 => vec![lr],
        0x2598 => vec![ul],
        0x2599 => vec![ul, ll, lr],
        0x259a => vec![ul, lr],
        0x259b => vec![ul, ur, ll],
        0x259c => vec![ul, ur, lr],
        0x259d => vec![ur],
        0x259e => vec![ur, ll],
        0x259f => vec![ur, ll, lr],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ColorPair;
    #[test]
    fn grid_fits_the_monitor_and_blocks_tile() {
        let mut r = Renderer::new(1920, 1080, 1.0);
        assert!(r.columns >= 120 && r.rows >= 30, "{}x{}", r.columns, r.rows);
        let mut cells = vec![
            Cell {
                symbol: ' ',
                colors: ColorPair::default()
            };
            r.columns * r.rows
        ];
        cells[0] = Cell {
            symbol: '█',
            colors: ColorPair::fg(Color::hex("ffffff")),
        };
        cells[1] = cells[0];
        assert!(r.draw(&cells));
        assert!(!r.draw(&cells));
        let (x, y) = (r.offset_x + r.cell_width, r.offset_y + r.cell_height / 2);
        let i = (y * r.width + x) * 3;
        assert_eq!(&r.pixels()[i - 3..i + 3], &[255; 6]);
    }
}
