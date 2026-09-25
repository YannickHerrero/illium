//! Read-only benchmark: profile_wallpaper <wallpaper-directory> [width height].
use std::{path::PathBuf, time::Instant};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.first().ok_or("expected wallpaper directory")?);
    let width = args
        .get(1)
        .map_or(Ok(2560), |s| s.parse::<u32>())
        .map_err(|e| e.to_string())?;
    let height = args
        .get(2)
        .map_or(Ok(1600), |s| s.parse::<u32>())
        .map_err(|e| e.to_string())?;
    for name in illium_theme::pack::images(&dir)? {
        let start = Instant::now();
        let pixels = illium_theme::pack::decode(&dir.join(&name))?;
        let decoded = start.elapsed();
        let fit = Instant::now();
        let result = illium_theme::pack::cover(&pixels, width, height)?;
        println!(
            "{name}: {}x{} -> {}x{}; decode={} ms, fit={} ms, total={} ms",
            pixels.width(),
            pixels.height(),
            result.width(),
            result.height(),
            decoded.as_millis(),
            fit.elapsed().as_millis(),
            start.elapsed().as_millis()
        );
    }
    Ok(())
}
