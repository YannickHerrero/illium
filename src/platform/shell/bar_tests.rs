//! Render the actual bar offscreen: only its single base layer may fade.
use super::*;
use slint::platform::{
    Platform, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use std::cell::RefCell;

struct Headless(Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.0.borrow_mut() = Some(window.clone());
        Ok(window)
    }
}
#[test]
fn bar_fades_only_its_background_and_honors_shared_override() {
    let window = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let bar = Bar::new().unwrap();
    bar.set_surface_width(500.);
    bar.set_surface_height(40.);
    bar.set_bg(color("#0a1c33"));
    bar.set_fg(color("#eaf6ff"));
    bar.set_muted(color("#8fa8c4"));
    bar.set_accent(color("#9fe0ff"));
    bar.set_workspaces(ModelRc::new(VecModel::from(vec![1, 2])));
    bar.set_active(1);
    bar.show().unwrap();
    let window = window.borrow().as_ref().unwrap().clone();
    window.dispatch_event(WindowEvent::Resized {
        size: slint::LogicalSize::new(500., 40.),
    });
    let pixels = || {
        window.request_redraw();
        let mut pixels = vec![PremultipliedRgbaColor::default(); 500 * 40];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 500);
        });
        pixels
    };
    for (opacity, expected) in [(0., 0), (0.5, 128), (0.85, 217), (1., 255)] {
        bar.set_background_opacity(opacity);
        let image = pixels();
        assert!((i32::from(image[250].alpha) - expected).abs() <= 1);
        // Empty corner of the inactive workspace must not add another layer.
        assert!((i32::from(image[5 * 500 + 38].alpha) - expected).abs() <= 1);
        assert_eq!(image[5 * 500 + 5].alpha, 255, "active indicator faded");
        assert_eq!(bar.get_fg().alpha(), 255);
        assert_eq!(bar.get_muted().alpha(), 255);
        assert_eq!(
            bar.get_bg().alpha(),
            255,
            "active label color must stay opaque"
        );
    }
    bar.set_background_opacity(0.85);
    bar.set_transparent(true);
    assert_eq!(pixels()[250].alpha, 0);
    bar.set_transparent(false);
    assert!((i32::from(pixels()[250].alpha) - 217).abs() <= 1);

    let home =
        std::env::temp_dir().join(format!("winarchy-bar-opacity-test-{}", std::process::id()));
    Config::install(&home).unwrap();
    let config = Config::load(&home).unwrap();
    assert_eq!(
        Shell::background_opacity(&config),
        config.theme.background_opacity
    );
    winarchy_theme::opacity::set(&home, &config.global.theme, 0.6).unwrap();
    assert_eq!(Shell::background_opacity(&config), 0.6);
    assert_eq!(config.theme.background_opacity, 0.85);
    winarchy_theme::opacity::clear(&home).unwrap();
    assert_eq!(Shell::background_opacity(&config), 0.85);
    std::fs::remove_dir_all(home).unwrap();
}
