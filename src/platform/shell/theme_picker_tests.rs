//! Production picker exercised with Slint's software renderer and no HWND.
//! No changes to the running desktop, current configuration or active theme.
use super::*;
use slint::Model as _;
use slint::platform::{
    Platform, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use std::{
    fs,
    time::{Duration, Instant},
};

struct Headless(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn settle(picker: &mut Picker) -> Outcome {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let result = picker.poll();
        if result != Outcome::None || !picker.loading() {
            return result;
        }
        assert!(Instant::now() < until, "preview worker timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn snapshot(window: &MinimalSoftwareWindow, name: &str) -> Vec<PremultipliedRgbaColor> {
    window.request_redraw();
    let size = window.size();
    let mut pixels = vec![PremultipliedRgbaColor::default(); (size.width * size.height) as usize];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, size.width as usize);
    });
    if let Some(dir) = std::env::var_os("WINARCHY_PICKER_RENDER_DIR") {
        fs::create_dir_all(&dir).unwrap();
        let image = image::RgbaImage::from_fn(size.width, size.height, |x, y| {
            let p = pixels[(y * size.width + x) as usize];
            let unpremultiply = |c: u8| {
                if p.alpha == 0 {
                    0
                } else {
                    (u32::from(c) * 255 / u32::from(p.alpha)).min(255) as u8
                }
            };
            image::Rgba([
                unpremultiply(p.red),
                unpremultiply(p.green),
                unpremultiply(p.blue),
                p.alpha,
            ])
        });
        image
            .save(PathBuf::from(dir).join(format!("{name}.png")))
            .unwrap();
    }
    pixels
}
#[test]
fn headless_picker_renders_filters_and_never_applies_while_browsing() {
    let temp =
        Temp(std::env::temp_dir().join(format!("winarchy-picker-ui-{}", std::process::id())));
    let _ = fs::remove_dir_all(&temp.0);
    Config::install(&temp.0).unwrap();
    // Test legacy palette-only themes alongside our deterministic fixtures.
    for id in ["catppuccin-mocha", "catppuccin-latte"] {
        fs::remove_dir_all(temp.0.join("themes").join(id)).unwrap();
    }
    for (id, color) in [
        ("amber", [210, 140, 40, 255]),
        ("ocean", [30, 110, 180, 255]),
        ("tokyo-night", [110, 60, 170, 255]),
    ] {
        fs::write(
            temp.0.join(format!("themes/{id}.toml")),
            include_str!("../../../config/themes/catppuccin-mocha.toml"),
        )
        .unwrap();
        fs::create_dir_all(temp.0.join(format!("themes/{id}"))).unwrap();
        image::RgbaImage::from_fn(160, 90, |x, y| {
            if x % 40 < 2 || y % 30 < 2 {
                image::Rgba([240, 240, 240, 255])
            } else {
                image::Rgba(color)
            }
        })
        .save(temp.0.join(format!("themes/{id}/preview.png")))
        .unwrap();
    }
    fs::write(temp.0.join("winarchy.toml"), "theme = \"ocean\"\n").unwrap();
    let before = fs::read(temp.0.join("winarchy.toml")).unwrap();
    let config = Config::load(&temp.0).unwrap();
    let windows = Rc::new(RefCell::new(vec![]));
    slint::platform::set_platform(Box::new(Headless(windows.clone()))).unwrap();
    let (tx, rx) = crate::queue::channel(1024);
    let mut picker = Picker::new(tx).unwrap();
    let monitor = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    for _ in 0..2 {
        picker.preload(&config, monitor, None);
        let until = Instant::now() + Duration::from_secs(30);
        while picker.warming.is_some() {
            assert_eq!(picker.poll(), Outcome::None);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(!picker.opened, "preload must not open or focus the picker");
    assert_eq!(picker.rows.row_count(), 0);
    assert_eq!(picker.views.len(), 2, "both initial contexts are warmed");
    picker.preload(&config, monitor, None);
    assert!(picker.warming.is_none(), "idle polling does not repeat completed work");
    picker.open(&config, monitor, None).unwrap();
    assert!(picker.ui.get_content_ready(), "preloaded frames display without waiting for the worker");
    // Use a deterministic 96-DPI monitor, independently of the host's actual DPI.
    picker.ui.set_surface_width(1920.0);
    picker.ui.set_surface_height(1080.0);
    let window = windows.borrow().last().unwrap().clone();
    window.set_size(slint::PhysicalSize::new(1920, 1080));
    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    picker.ui.invoke_focus_picker();
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.selected_id(), Some("ocean"));
    assert_eq!(
        picker.model.ids.len(),
        3,
        "themes without assets get no placeholder"
    );
    let pixels = snapshot(&window, "picker-center");
    assert!(
        pixels[0].alpha.abs_diff(128) <= 1,
        "scrim must retain per-pixel alpha"
    );
    assert!(
        pixels[(400 * 1920 + 900) as usize].alpha > 250,
        "central card stays opaque"
    );
    let uploaded = picker.rows.row_data(2).unwrap().image;
    picker.render();
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(
        picker.rows.row_data(2).unwrap().image,
        uploaded,
        "warm redraw reuses the Slint image"
    );
    let epoch = picker.epoch.get();
    picker.input(epoch, Input::Action(Action::Next));
    assert_eq!(picker.input(epoch, Input::Click(960.0, 500.0)), Outcome::None);
    assert_eq!(settle(&mut picker), Outcome::Apply("ocean".into()));
    assert_eq!(picker.rows.row_data(2).unwrap().image, uploaded,
        "clicking the old visible center must not publish the queued keyboard image");
    picker.input(epoch, Input::Action(Action::Text("o".into())));
    assert_eq!(settle(&mut picker), Outcome::None);
    snapshot(&window, "picker-filter-multiple");
    picker.input(epoch, Input::Action(Action::Clear));
    assert_eq!(settle(&mut picker), Outcome::None);
    picker.ui.set_selected_label("A very long theme name — with accents éàç — repeated to test right elision across the full preview width — and more text".into());
    snapshot(&window, "picker-long-label");
    // Drive the real FocusScope/callback/queue path, not just the pure model.
    for c in "tokyo n".chars() {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: c.to_string().into(),
        });
        window.dispatch_event(WindowEvent::KeyReleased {
            text: c.to_string().into(),
        });
    }
    for event in rx.try_iter() {
        if let Event::Picker(e, input) = event {
            assert_eq!(picker.input(e, input), Outcome::None);
        }
    }
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.selected_id(), Some("tokyo-night"));
    assert_eq!(picker.filter(), "tokyo n");
    snapshot(&window, "picker-filter-one");
    picker.input(epoch, Input::Action(Action::Text("zzz".into())));
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.ui.get_selected_label(), "No matches");
    snapshot(&window, "picker-no-matches");
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Confirm)),
        Outcome::Cancel
    );
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Escape)),
        Outcome::None
    );
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Previous)),
        Outcome::None
    );
    assert_eq!(settle(&mut picker), Outcome::None);
    snapshot(&window, "picker-previous");
    assert_eq!(picker.input(epoch, Input::Click(0.0, 0.0)), Outcome::Cancel);
    // Confirmation is deferred until the matching image arrives, never applied
    // to the previous visible card or a replacement of a failed image.
    picker.input(epoch, Input::Action(Action::Next));
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Confirm)),
        Outcome::None
    );
    assert_eq!(settle(&mut picker), Outcome::Apply("tokyo-night".into()));
    for (dpi, scale) in [(120, 1.25), (144, 1.5), (192, 2.0)] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        window.set_size(slint::PhysicalSize::new(
            (1920.0 * scale) as u32,
            (1080.0 * scale) as u32,
        ));
        picker.monitor.w = (1920.0 * scale) as i32;
        picker.monitor.h = (1080.0 * scale) as i32;
        picker.render();
        assert_eq!(settle(&mut picker), Outcome::None);
        snapshot(&window, &format!("picker-dpi-{dpi}"));
    }
    fs::write(temp.0.join("themes/tokyo-night/preview.png"), "broken").unwrap();
    picker.rescan();
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Confirm)),
        Outcome::None
    );
    assert_eq!(
        settle(&mut picker),
        Outcome::None,
        "a failed target must not confirm its replacement"
    );
    assert!(!picker.model.ids.contains(&"tokyo-night".into()));
    assert!(picker.error.is_some());
    let mut light = config.clone();
    light.theme =
        winarchy_theme::Theme::parse(include_str!("../../../config/themes/catppuccin-latte.toml"))
            .unwrap();
    picker.apply_theme(&light);
    assert_eq!(settle(&mut picker), Outcome::None);
    snapshot(&window, "picker-light");
    picker.rescan();
    picker.close();
    picker.open(&config, monitor, None).unwrap();
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Confirm)),
        Outcome::None,
        "stale opening input must be ignored"
    );
    assert_eq!(settle(&mut picker), Outcome::None);
    picker.close();
    assert_eq!(fs::read(temp.0.join("winarchy.toml")).unwrap(), before);
    assert!(!temp.0.join("wallpapers.json").exists());

    // The same production surface/worker now browses actual wallpapers. Exact
    // filenames, theme scoping and deferred confirmation remain authoritative.
    let dir = temp.0.join("themes/ocean/wallpapers");
    fs::create_dir_all(&dir).unwrap();
    for name in ["A painting.png", "Été - 2.png"] {
        image::RgbaImage::from_pixel(160, 90, image::Rgba([40, 120, 180, 255]))
            .save(dir.join(name))
            .unwrap();
    }
    picker
        .open_wallpapers(&config, monitor, Some(123), Some("Été - 2.png"))
        .unwrap();
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.selected_id(), Some("Été - 2.png"));
    assert_eq!(picker.ui.get_selected_label(), "Été - 2.png");
    assert_eq!(picker.model.ids.len(), 2);
    snapshot(&window, "wallpaper-picker");
    let epoch = picker.epoch.get();
    picker.input(epoch, Input::Action(Action::Next));
    picker.input(epoch, Input::Action(Action::Confirm));
    assert_eq!(settle(&mut picker), Outcome::Apply("A painting.png".into()));
    assert_eq!(
        picker.selection_command("A painting.png".into(), "ocean"),
        Some(crate::command::Command::Wallpaper(Some(
            "A painting.png".into()
        )))
    );
    assert_eq!(
        picker.selection_command("A painting.png".into(), "amber"),
        None
    );
    picker.input(epoch, Input::Action(Action::Text("été".into())));
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.selected_id(), Some("Été - 2.png"));
    // A removed selected image must not confirm its replacement on rescan.
    fs::remove_file(dir.join("Été - 2.png")).unwrap();
    picker.rescan();
    picker.input(epoch, Input::Action(Action::Confirm));
    assert_eq!(settle(&mut picker), Outcome::None);
    picker.input(epoch, Input::Action(Action::Clear));
    assert_eq!(settle(&mut picker), Outcome::None);
    assert_eq!(picker.selected_id(), Some("A painting.png"));
    let mut switched = config.clone();
    switched.global.theme = "amber".into();
    picker.apply_theme(&switched);
    assert_eq!(
        picker.input(epoch, Input::Action(Action::Confirm)),
        Outcome::Cancel
    );
    assert_eq!(picker.poll(), Outcome::Cancel);
    assert_eq!(picker.close(), Some(123));
    picker
        .open_wallpapers(&switched, monitor, None, None)
        .unwrap();
    assert_eq!(settle(&mut picker), Outcome::None);
    assert!(
        picker.model.ids.is_empty(),
        "no placeholders or images from another theme"
    );
    assert_eq!(
        picker.input(picker.epoch.get(), Input::Action(Action::Escape)),
        Outcome::Cancel
    );
    picker.close();
    picker.open(&config, monitor, None).unwrap();
    assert_eq!(settle(&mut picker), Outcome::None);
    assert!(picker.wallpaper_theme.is_none());
    assert_eq!(picker.selected_id(), Some("ocean"));
    let image = picker.rows.row_data(picker.rows.row_count() - 1).unwrap().image;
    picker.close();
    picker.open(&config, monitor, None).unwrap();
    assert!(picker.ui.get_content_ready(), "warm view is visible before polling");
    assert!(picker.loading(), "cached pixels still require catalog validation");
    assert_eq!(picker.rows.row_data(picker.rows.row_count() - 1).unwrap().image, image);
    assert_eq!(picker.input(picker.epoch.get(), Input::Action(Action::Confirm)), Outcome::None);
    assert_eq!(settle(&mut picker), Outcome::Apply("ocean".into()));
    picker.close();
    fs::remove_file(temp.0.join("themes/ocean.toml")).unwrap();
    picker.open(&config, monitor, None).unwrap();
    picker.input(picker.epoch.get(), Input::Action(Action::Confirm));
    assert_eq!(settle(&mut picker), Outcome::None, "removed cached target cannot apply");
    assert_ne!(picker.selected_id(), Some("ocean"));
    picker.close();
    picker.rescan();
    assert!(picker.views.is_empty(), "notifications invalidate closed views too");
    assert_eq!(fs::read(temp.0.join("winarchy.toml")).unwrap(), before);
    assert!(!temp.0.join("wallpapers.json").exists());
}
