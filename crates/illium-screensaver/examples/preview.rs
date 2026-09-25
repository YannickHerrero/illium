//! Renders an effect run to PNG contact sheets for visual review:
//! `cargo run -p illium-screensaver --example preview -- <effect> <out-dir> [width height scale] [reference.cells]`
//! With a `.cells` dump of the Python TTE run (same grid), the sheet shows
//! Illium frames on the left and the reference frames on the right.
use illium_config::screensaver::Effect;
use illium_screensaver::engine::{Cell, Color, ColorPair};
use illium_screensaver::player::{Player, TICKS_PER_SECOND};
use illium_screensaver::render::Renderer;
use std::fs::File;
use std::io::BufWriter;

/// Parses `tte_ref.py` output: a header then `codepoint fg bg` per cell.
fn reference(path: &str, width: usize, height: usize, scale: f32) -> (usize, Vec<Vec<u8>>) {
    let text = std::fs::read_to_string(path).expect("read reference");
    let mut lines = text.lines();
    let header: Vec<usize> = lines
        .next()
        .unwrap()
        .split(' ')
        .map(|n| n.parse().unwrap())
        .collect();
    let (total, cols, rows, count) = (header[0], header[1], header[2], header[3]);
    let mut renderer = Renderer::new(width, height, scale);
    assert_eq!(
        (renderer.columns, renderer.rows),
        (cols, rows),
        "reference grid differs"
    );
    let color = |s: &str| (s != "-").then(|| Color::hex(s));
    let mut frames = vec![];
    for _ in 0..count {
        let cells: Vec<Cell> = (0..cols * rows)
            .map(|_| {
                let mut parts = lines.next().unwrap().split(' ');
                let symbol = char::from_u32(parts.next().unwrap().parse().unwrap()).unwrap();
                let fg = color(parts.next().unwrap());
                let bg = color(parts.next().unwrap());
                Cell {
                    symbol,
                    colors: ColorPair::new(fg, bg),
                }
            })
            .collect();
        renderer.draw(&cells);
        frames.push(renderer.pixels().to_vec());
    }
    (total, frames)
}

fn save(path: &str, width: usize, height: usize, rgb: &[u8]) {
    let file = BufWriter::new(File::create(path).expect("create png"));
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgb)
        .unwrap();
}

/// Box-downscales by `factor`.
fn shrink(width: usize, height: usize, rgb: &[u8], factor: usize) -> (usize, usize, Vec<u8>) {
    let (w, h) = (width / factor, height / factor);
    let mut out = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut sum = 0u32;
                for dy in 0..factor {
                    for dx in 0..factor {
                        sum += rgb[((y * factor + dy) * width + x * factor + dx) * 3 + c] as u32;
                    }
                }
                out[(y * w + x) * 3 + c] = (sum / (factor * factor) as u32) as u8;
            }
        }
    }
    (w, h, out)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).expect("effect name");
    let out = args.get(2).expect("output directory");
    let width: usize = args.get(3).map_or(1920, |a| a.parse().unwrap());
    let height: usize = args.get(4).map_or(1080, |a| a.parse().unwrap());
    let scale: f32 = args.get(5).map_or(1.0, |a| a.parse().unwrap());
    let effect = *Effect::ALL
        .iter()
        .find(|e| e.name() == name)
        .expect("unknown effect");
    let player = || {
        let mut player = Player::new(&[effect], width, height, scale, 42);
        player.shuffle = false;
        player.start(effect);
        player
    };
    // Same seed twice: count the frames, then capture evenly spaced ones.
    let mut counting = player();
    let started = std::time::Instant::now();
    let mut total = 0;
    while counting.step_frame() {
        total += 1;
        assert!(total < 60_000, "effect never finished");
    }
    let elapsed = started.elapsed();
    println!(
        "{name}: {total} frames = {:.1} s at 120 fps; simulated+rendered in {:.2} s ({:.2} ms/frame)",
        total as f64 / TICKS_PER_SECOND,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / total.max(1) as f64
    );
    let picks = 12;
    let wanted: Vec<usize> = (0..picks).map(|i| (total - 1) * i / (picks - 1)).collect();
    let mut capture = player();
    let mut frames = vec![];
    for frame in 0..total {
        capture.step_frame();
        if wanted.contains(&frame) {
            frames.push(capture.pixels().to_vec());
        }
    }
    let (w, h) = capture.size();
    let grid = Renderer::new(width, height, scale);
    println!("grid {}x{}", grid.columns, grid.rows);
    save(
        &format!("{out}/{name}-final.png"),
        w,
        h,
        frames.last().unwrap(),
    );
    let mut tiles = frames.clone();
    let mut cols = 3;
    if let Some(path) = args.get(6) {
        let (reference_total, reference_frames) = reference(path, width, height, scale);
        println!(
            "reference: {reference_total} frames = {:.1} s",
            reference_total as f64 / TICKS_PER_SECOND
        );
        let last = reference_frames.last().unwrap();
        save(&format!("{out}/{name}-reference-final.png"), w, h, last);
        let paired = reference_frames.iter().chain(std::iter::repeat(last));
        tiles = frames
            .iter()
            .zip(paired)
            .flat_map(|(a, b)| [a.clone(), b.clone()])
            .collect();
        cols = 2;
    }
    let factor = 3;
    let (tw, th) = (w / factor, h / factor);
    let rows = tiles.len().div_ceil(cols);
    let sheet_w = cols * (tw + 4);
    let mut sheet = vec![40u8; sheet_w * (rows * (th + 4)) * 3];
    for (i, tile) in tiles.iter().enumerate() {
        let (_, _, thumb) = shrink(w, h, tile, factor);
        let (ox, oy) = ((i % cols) * (tw + 4) + 2, (i / cols) * (th + 4) + 2);
        for y in 0..th {
            let dst = ((oy + y) * sheet_w + ox) * 3;
            sheet[dst..dst + tw * 3].copy_from_slice(&thumb[y * tw * 3..(y + 1) * tw * 3]);
        }
    }
    save(
        &format!("{out}/{name}-sheet.png"),
        sheet_w,
        rows * (th + 4),
        &sheet,
    );
}
