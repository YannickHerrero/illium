//! Exercise the actual Slint layout, without native windows.
use super::{Bar, BarHint, BarHints, StatusItem};
use slint::platform::{
    Platform, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

struct Headless(Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.0.borrow_mut() = Some(window.clone());
        Ok(window)
    }
}

#[test]
fn hint_centers_follow_layout_and_only_report_actionable_modules() {
    let adapter = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Headless(adapter.clone()))).unwrap();
    let bar = Bar::new().unwrap();
    bar.set_surface_width(800.);
    bar.set_surface_height(36.);
    bar.set_workspaces(ModelRc::new(VecModel::from(vec![1, 3])));
    let item = |kind: &str, hint_id| StatusItem {
        kind: kind.into(),
        value: "test".into(),
        hint_id,
        ..Default::default()
    };
    bar.set_left_items(ModelRc::new(VecModel::from(vec![
        item("weather", 1),
        item("separator", 0),
    ])));
    bar.set_center_items(ModelRc::new(VecModel::from(vec![item("clock", 2)])));
    bar.set_right_items(ModelRc::new(VecModel::from(vec![
        item("window-title", 0),
        item("wifi", 3),
    ])));
    let reports = Rc::new(RefCell::new(vec![]));
    let results = reports.clone();
    bar.on_hint_position(move |epoch, index, x| results.borrow_mut().push((epoch, index, x)));
    bar.show().unwrap();
    let window = adapter.borrow().as_ref().unwrap().clone();
    window.dispatch_event(WindowEvent::Resized {
        size: slint::LogicalSize::new(800., 36.),
    });
    let draw = || {
        window.request_redraw();
        let mut pixels = vec![PremultipliedRgbaColor::default(); 800 * 36];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 800);
        });
        slint::platform::update_timers_and_animations();
    };
    draw();
    assert!(reports.borrow().is_empty());
    bar.set_hint_request(7);
    draw();
    let mut first = reports.borrow().clone();
    first.sort_by_key(|(_, index, _)| *index);
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|(epoch, _, _)| *epoch == 7));
    assert_eq!(
        first.iter().map(|(_, i, _)| *i).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(first[0].2 > 50. && first[0].2 < 200.);
    assert!((first[1].2 - 400.).abs() < 1.);
    assert!(first[2].2 > 600. && first[2].2 < 800.);
    // Reopening must report again even though module geometry did not change.
    reports.borrow_mut().clear();
    bar.set_hint_request(8);
    draw();
    assert_eq!(reports.borrow().len(), 3);
    assert!(reports.borrow().iter().all(|(epoch, _, _)| *epoch == 8));

    let hints = BarHints::new().unwrap();
    hints.set_surface_width(800.);
    hints.set_bg(slint::Color::from_rgb_u8(20, 30, 40));
    hints.set_fg(slint::Color::from_rgb_u8(240, 240, 240));
    hints.set_accent(slint::Color::from_rgb_u8(100, 200, 255));
    hints.set_hints(ModelRc::new(VecModel::from(vec![BarHint {
        center: 400.,
        label: "1".into(),
    }])));
    hints.show().unwrap();
    let window = adapter.borrow().as_ref().unwrap().clone();
    window.dispatch_event(WindowEvent::Resized {
        size: slint::LogicalSize::new(800., 26.),
    });
    let mut pixels = vec![PremultipliedRgbaColor::default(); 800 * 26];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    assert_eq!(
        pixels[12 * 800 + 50].alpha,
        0,
        "hint strip must not cover desktop"
    );
    assert_eq!(
        pixels[12 * 800 + 395].alpha,
        255,
        "hint badge must stay opaque"
    );
}

#[cfg(windows)]
#[test]
fn hint_sessions_reject_stale_input_and_wait_for_all_centers() {
    use super::bar_hints::Hints;
    use crate::{bar_hints::Input, config::Config, layout::Rect};
    let adapter = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Headless(adapter))).unwrap();
    let home = std::env::temp_dir().join(format!("winarchy-hints-test-{}", std::process::id()));
    Config::install(&home).unwrap();
    let c = Config::load(&home).unwrap();
    let r = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let mut hints = Hints::new().unwrap();
    hints.open(&c, r, 2, vec![]);
    assert!(!hints.opened);
    hints.open(&c, r, 2, vec!["right".into(), "left".into()]);
    let first = hints.generation;
    assert_eq!(hints.input(first, Input::Select(0)), None);
    hints.position(first, 0, 700.);
    hints.position(first - 1, 1, 100.);
    assert_eq!(hints.input(first, Input::Accept), None);
    hints.position(first, 1, 100.);
    assert_eq!(hints.input(first, Input::Select(34)), None);
    assert!(hints.opened);
    hints.input(first, Input::Previous);
    assert_eq!(
        hints.input(first, Input::Accept),
        Some(("right".into(), 700, 2))
    );
    assert!(!hints.opened);
    hints.open(&c, r, 0, vec!["new".into()]);
    hints.position(first, 0, 100.);
    assert_eq!(hints.input(first, Input::Cancel), None);
    assert!(hints.opened);
    hints.input(hints.generation, Input::Cancel);
    assert!(
        !hints.opened,
        "Escape must also cancel before geometry is ready"
    );
    std::fs::remove_dir_all(home).unwrap();
}
