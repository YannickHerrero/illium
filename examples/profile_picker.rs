//! CPU-side picker benchmark; never applies a theme or wallpaper.
//! profile_picker <config-home> [wallpaper-theme]
//! Disk cache is used normally; use an isolated cache directory for cold tests.
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use winarchy::theme_picker::{
    Model,
    loader::{Job, Loader, Output},
    render::{Colors, Key},
};

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let home = PathBuf::from(
        args.next()
            .ok_or("usage: profile_picker <config-home> [wallpaper-theme]")?,
    );
    let theme = args.next().map(|s| s.to_string_lossy().into_owned());
    let started = Instant::now();
    let entries = match &theme {
        Some(theme) => winarchy_theme::preview::wallpapers(&home, theme)?,
        None => winarchy_theme::preview::catalog(&home)?,
    };
    println!(
        "catalog: {} entries, {:.2} ms",
        entries.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    let active = entries
        .get(entries.len() / 2)
        .map(|e| e.id.as_str())
        .unwrap_or_default();
    let model = Model::new(entries.iter().map(|e| e.id.clone()).collect(), active);
    let colors = Colors::from_theme(&winarchy_theme::Theme::default_theme());
    let cards = model.cards(1920.0, 1080.0);
    let keys: Vec<_> = cards
        .iter()
        .map(|card| Key {
            entry: entries[card.index].clone(),
            dpi: 96,
            selected: card.selected,
            colors,
        })
        .collect();
    let loader = Loader::default();
    for pass in ["RAM cold", "RAM warm"] {
        let started = Instant::now();
        let mut first = None;
        loader.request(Job::Render(keys.clone()));
        loop {
            if started.elapsed() > Duration::from_secs(60) {
                return Err("picker timed out".into());
            }
            let Some(result) = loader.take_result() else {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            match result.result? {
                Output::Progress(frames) => {
                    if frames.iter().any(Option::is_some) {
                        first.get_or_insert(started.elapsed());
                    }
                }
                Output::Frames(frames) => {
                    let total = started.elapsed();
                    println!(
                        "{pass}: {} cards, first pixels {:.2} ms, complete {:.2} ms, {:.2} MiB",
                        frames.len(),
                        first.unwrap_or(total).as_secs_f64() * 1000.0,
                        total.as_secs_f64() * 1000.0,
                        frames.iter().map(|f| f.bytes()).sum::<usize>() as f64 / (1024.0 * 1024.0)
                    );
                    break;
                }
                Output::Unreadable(entry, error) => {
                    return Err(format!("{}: {error}", entry.path.display()));
                }
                Output::Catalog(_) => return Err("unexpected catalog".into()),
            }
        }
    }
    Ok(())
}
