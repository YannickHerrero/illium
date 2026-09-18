//! Render the actual bar offscreen: opacity, module order and click anchors.
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
fn left_sections_preserve_configured_workspace_position() {
    for (input, before, after) in [
        (
            vec!["pet", "workspaces", "clock"],
            vec!["pet"],
            vec!["clock"],
        ),
        (vec!["workspaces", "pet"], vec![], vec!["pet"]),
        (
            vec!["pet", "separator", "workspaces"],
            vec!["pet", "separator"],
            vec![],
        ),
        (vec!["pet", "clock"], vec![], vec!["pet", "clock"]),
        (vec![], vec![], vec![]),
    ] {
        let modules = input.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (actual_before, actual_after) = split_workspaces(&modules);
        assert_eq!(actual_before, before, "{input:?}");
        assert_eq!(actual_after, after, "{input:?}");
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
        assert!((i32::from(image[12 * 500 + 44].alpha) - expected).abs() <= 1);
        assert_eq!(image[12 * 500 + 14].alpha, 255, "active indicator faded");
        assert_eq!(bar.get_fg().alpha(), 255);
        assert_eq!(bar.get_muted().alpha(), 255);
        assert_eq!(
            bar.get_bg().alpha(),
            255,
            "active label color must stay opaque"
        );
    }
    // The visual marker is 24×22, centered inside the unchanged 26px hit target.
    let image = pixels();
    let accent = bar.get_accent();
    let marker: Vec<_> = image
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            (p.red == accent.red() && p.green == accent.green() && p.blue == accent.blue())
                .then_some((i % 500, i / 500))
        })
        .collect();
    assert_eq!(marker.iter().map(|p| p.0).min(), Some(11));
    assert_eq!(marker.iter().map(|p| p.0).max(), Some(34));
    assert_eq!(marker.iter().map(|p| p.1).min(), Some(9));
    assert_eq!(marker.iter().map(|p| p.1).max(), Some(30));
    bar.set_background_opacity(0.85);
    bar.set_transparent(true);
    assert_eq!(pixels()[250].alpha, 0);
    bar.set_transparent(false);
    assert!((i32::from(pixels()[250].alpha) - 217).abs() <= 1);

    // Modules before and after the workspace group keep their positions and
    // report the correct anchors. The active workspace is no longer at the left inset.
    bar.set_before_workspace_items(ModelRc::new(VecModel::from(vec![StatusItem {
        kind: "pet".into(),
        value: "Pet".into(),
        ..Default::default()
    }])));
    bar.set_left_items(ModelRc::new(VecModel::from(vec![StatusItem {
        kind: "clock".into(),
        value: "After".into(),
        ..Default::default()
    }])));
    let image = pixels();
    let accent = bar.get_accent();
    let active_x = (0..500)
        .find(|x| {
            let pixel = image[12 * 500 + x];
            pixel.red == accent.red()
                && pixel.green == accent.green()
                && pixel.blue == accent.blue()
        })
        .unwrap();
    assert!(
        active_x > 25,
        "workspace still rendered before the leading module"
    );
    let modules = Rc::new(RefCell::new(Vec::new()));
    let clicked = modules.clone();
    bar.on_module(move |name, anchor| clicked.borrow_mut().push((name.to_string(), anchor)));
    let selected = Rc::new(RefCell::new(0));
    let clicked = selected.clone();
    bar.on_workspace(move |n| *clicked.borrow_mut() = n);
    let click = |x| {
        let position = slint::LogicalPosition::new(x, 20.);
        window.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    };
    click(16.);
    assert_eq!(modules.borrow()[0].0, "pet");
    assert!(modules.borrow()[0].1 < active_x as f32);
    click(active_x as f32 + 5.);
    assert_eq!(*selected.borrow(), 1);
    click(active_x as f32 + 56. + 10.);
    assert_eq!(modules.borrow()[1].0, "clock");
    assert!(modules.borrow()[1].1 > active_x as f32 + 52.);

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
