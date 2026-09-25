//! Production wallpaper coordinator without HWNDs or changes to the desktop.
use super::*;
use slint::platform::{
    Platform, WindowAdapter,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
struct Headless;
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
    }
}
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn settle(shell: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while shell.pending_wallpaper().is_some() {
        shell.poll_wallpaper();
        assert!(Instant::now() < deadline, "wallpaper worker timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn dynamic_wallpaper_lifecycle_is_latest_wins_and_failure_safe() {
    slint::platform::set_platform(Box::new(Headless)).unwrap();
    let temp = Temp(std::env::temp_dir().join(format!("illium-dynamic-ui-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&temp.0);
    Config::install(&temp.0).unwrap();
    std::fs::write(temp.0.join("illium.toml"), "theme = 'dynamic-dark'").unwrap();
    let (tx, _) = crate::queue::channel(1024);
    let mut shell = Shell::new(tx).unwrap();
    shell.wallpaper_sizes = vec![(16, 9), (9, 16)];
    shell.apply_theme(&Config::load(&temp.0).unwrap());
    assert!(shell.wallpaper.is_none()); // empty library: no hidden image card
    let dir = temp.0.join("wallpapers/dynamic");
    for (name, rgba) in [("a.png", [250, 40, 20, 255]), ("b.png", [20, 40, 250, 255])] {
        image::RgbaImage::from_pixel(32, 32, image::Rgba(rgba))
            .save(dir.join(name))
            .unwrap();
    }
    shell.refresh_wallpaper();
    settle(&mut shell);
    assert_eq!(shell.wallpaper.as_deref(), Some("a.png"));
    let first = illium_theme::Theme::effective(&temp.0).unwrap();
    illium_theme::opacity::set(&temp.0, "dynamic-dark", 0.65).unwrap();
    shell.set_wallpaper(Some("b.png".into())).unwrap();
    shell.set_wallpaper(Some("a.png".into())).unwrap();
    shell.set_wallpaper(Some("b.png".into())).unwrap();
    settle(&mut shell);
    assert_eq!(shell.wallpaper.as_deref(), Some("b.png"));
    let second = illium_theme::Theme::effective(&temp.0).unwrap();
    assert_ne!(first.accent, second.accent);
    assert_eq!(second.background_opacity, 0.65);
    assert!(shell.take_wallpaper_palette_dirty());
    let saved = std::fs::read(temp.0.join("wallpapers.json")).unwrap();
    let state = std::fs::read(temp.0.join(illium_theme::dynamic::FILE)).unwrap();
    std::fs::write(dir.join("broken.png"), "broken").unwrap();
    shell.set_wallpaper(Some("broken.png".into())).unwrap();
    settle(&mut shell);
    assert!(shell.wallpaper_error.is_some());
    assert_eq!(shell.wallpaper.as_deref(), Some("b.png"));
    assert_eq!(
        saved,
        std::fs::read(temp.0.join("wallpapers.json")).unwrap()
    );
    assert_eq!(
        state,
        std::fs::read(temp.0.join(illium_theme::dynamic::FILE)).unwrap()
    );
    // Same image, different mode: shared selection and cached pair.
    std::fs::write(temp.0.join("illium.toml"), "theme = 'dynamic-light'").unwrap();
    shell.apply_theme(&Config::load(&temp.0).unwrap());
    settle(&mut shell);
    assert_eq!(shell.wallpaper.as_deref(), Some("b.png"));
    assert_eq!(
        illium_theme::Theme::effective(&temp.0)
            .unwrap()
            .mode
            .as_deref(),
        Some("light")
    );
    // A fresh coordinator restores both the shared selection and mode.
    let (tx, _) = crate::queue::channel(1024);
    let mut restarted = Shell::new(tx).unwrap();
    restarted.wallpaper_sizes = vec![(16, 9)];
    restarted.apply_theme(&Config::load(&temp.0).unwrap());
    settle(&mut restarted);
    assert_eq!(restarted.wallpaper.as_deref(), Some("b.png"));
    drop(restarted);
    // A vanished remembered image selects the next readable image and palette.
    std::fs::remove_file(dir.join("b.png")).unwrap();
    shell.refresh_wallpaper();
    settle(&mut shell);
    assert_eq!(shell.wallpaper.as_deref(), Some("a.png"));
    let a_palette = illium_theme::Theme::effective(&temp.0).unwrap();
    image::RgbaImage::from_pixel(33, 32, image::Rgba([40, 220, 50, 255]))
        .save(dir.join("a.png"))
        .unwrap();
    shell.refresh_wallpaper();
    settle(&mut shell);
    assert_ne!(
        a_palette.accent,
        illium_theme::Theme::effective(&temp.0).unwrap().accent
    );
    // Broken runtime state must not stop startup/config loading; a fresh worker
    // regenerates it rather than treating it as a broken installed palette.
    std::fs::write(temp.0.join(illium_theme::dynamic::FILE), "broken").unwrap();
    let recovered = Config::load(&temp.0).unwrap();
    assert_eq!(
        recovered.theme,
        illium_theme::Theme::load(&temp.0, "dynamic-light").unwrap()
    );
    // Clear cancels in-flight work and restores the matching fallback palette.
    shell.set_wallpaper(Some("a.png".into())).unwrap();
    shell.set_wallpaper(None).unwrap();
    settle(&mut shell);
    shell.poll_wallpaper();
    assert!(shell.wallpaper.is_none());
    assert!(!temp.0.join(illium_theme::dynamic::FILE).exists());
    assert_eq!(
        illium_theme::Theme::effective(&temp.0).unwrap(),
        illium_theme::Theme::load(&temp.0, "dynamic-light").unwrap()
    );
    shell.refresh_wallpaper();
    assert!(shell.wallpaper.is_none()); // explicit solid survives a reload
    // Static themes cancel work; late dynamic results cannot publish.
    shell.set_wallpaper(Some("a.png".into())).unwrap();
    std::fs::write(temp.0.join("illium.toml"), "theme = 'catppuccin-mocha'").unwrap();
    // Avoid decoding the large bundled wallpapers in this coordinator test.
    crate::wallpaper::Selections::default()
        .save_choice(&temp.0, "catppuccin-mocha", None)
        .unwrap();
    shell.apply_theme(&Config::load(&temp.0).unwrap());
    settle(&mut shell);
    std::thread::sleep(Duration::from_millis(100));
    shell.poll_wallpaper();
    assert!(shell.wallpaper.is_none());
    assert_eq!(
        illium_theme::Theme::effective(&temp.0).unwrap().name,
        "Catppuccin Mocha"
    );
}
