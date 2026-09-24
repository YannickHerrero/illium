//! Renders an effect run to PNG contact sheets for visual review:
//! `cargo run -p winarchy-screensaver --example preview -- <effect> <out-dir> [width height scale]`
use std::fs::File;
use std::io::BufWriter;
use winarchy_config::screensaver::Effect;
use winarchy_screensaver::player::{Player, TICKS_PER_SECOND};

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
    save(
        &format!("{out}/{name}-final.png"),
        w,
        h,
        frames.last().unwrap(),
    );
    let factor = 3;
    let (tw, th, _) = shrink(w, h, &frames[0], factor);
    let (cols, rows) = (3, picks.div_ceil(3));
    let mut sheet = vec![40u8; (cols * (tw + 4)) * (rows * (th + 4)) * 3];
    let sheet_w = cols * (tw + 4);
    for i in 0..picks {
        let (_, _, thumb) = shrink(w, h, &frames[i.min(frames.len() - 1)], factor);
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
