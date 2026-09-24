//! Production view/session without creating HWNDs or touching the user's desktop.
use super::*;
use slint::{
    Model as _,
    platform::{
        Platform, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
    },
};
use std::{cell::RefCell, time::Duration};

struct Headless(Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.0.borrow_mut() = Some(window.clone());
        Ok(window)
    }
}

#[test]
fn headless_workspace_switcher_navigation_rendering_and_lifecycle() {
    let adapter = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Headless(adapter.clone()))).unwrap();
    let (tx, _rx) = crate::queue::channel(64);
    let mut switcher = Switcher::new(tx).unwrap();
    let monitor = Rect {
        x: 0,
        y: 0,
        w: 1600,
        h: 1000,
    };
    let wallpaper = slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
        &[35_u8, 45, 85, 255].repeat(160 * 100),
        160,
        100,
    ));
    switcher.monitor = monitor;
    switcher.scene = Some(Scene {
        active: 5,
        backdrop: wallpaper.clone(),
        desktops: (1..=9)
            .map(|n| Desktop {
                monitor,
                wallpaper: wallpaper.clone(),
                windows: if n == 5 {
                    vec![
                        Window {
                            id: 1,
                            frame: Rect {
                                x: 100,
                                y: 100,
                                w: 1000,
                                h: 700,
                            },
                            source: Rect {
                                x: 0,
                                y: 0,
                                w: 1000,
                                h: 700,
                            },
                            live: false,
                            minimized: false,
                            title: "Terminal".into(),
                        },
                        Window {
                            id: 2,
                            frame: Rect {
                                x: 800,
                                y: 400,
                                w: 600,
                                h: 400,
                            },
                            source: Rect {
                                x: 0,
                                y: 0,
                                w: 600,
                                h: 400,
                            },
                            live: false,
                            minimized: false,
                            title: "Browser".into(),
                        },
                        Window {
                            id: 3,
                            frame: monitor,
                            source: monitor,
                            live: false,
                            minimized: true,
                            title: "Minimized".into(),
                        },
                    ]
                } else {
                    vec![]
                },
            })
            .collect(),
    });
    switcher.model = Model::new(5);
    switcher.opened = true;
    switcher.epoch.set(7);
    switcher.reveal = 1.0;
    switcher.restore = Some(123);
    let width = dpi::logical(monitor, monitor.w);
    let height = dpi::logical(monitor, monitor.h);
    switcher.ui.set_surface_width(width);
    switcher.ui.set_surface_height(height);
    switcher.ui.set_bg(color("#151525"));
    switcher.ui.set_fg(color("#dddddd"));
    switcher.ui.set_accent(color("#88bbff"));
    switcher.ui.set_backdrop(wallpaper);
    switcher.ui.set_ready(true);
    switcher.render();
    assert_eq!(switcher.rows.row_count(), 9);
    assert_eq!(switcher.rows.row_data(4).unwrap().label, "• 5");
    assert_eq!(switcher.rows.row_data(4).unwrap().detail, "3 windows");
    assert_eq!(switcher.rows.row_data(0).unwrap().detail, "Empty");
    let pieces: Vec<_> = switcher.placeholders.iter().collect();
    assert_eq!(
        pieces.len(),
        2,
        "minimized windows do not obscure the desktop"
    );
    assert_eq!(pieces[0].title, "Terminal");
    assert_eq!(
        pieces[1].title, "Browser",
        "fallbacks retain native paint order"
    );
    switcher.ui.show().unwrap();
    let window = adapter.borrow().as_ref().unwrap().clone();
    window.set_size(slint::PhysicalSize::new(
        width.round() as u32,
        height.round() as u32,
    ));
    window.dispatch_event(WindowEvent::Resized {
        size: slint::LogicalSize::new(width, height),
    });
    window.request_redraw();
    let (w, h) = (window.size().width as usize, window.size().height as usize);
    let mut pixels = vec![PremultipliedRgbaColor::default(); w * h];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, w);
    });
    assert!(
        pixels.iter().any(|p| p.blue > 180 && p.red > 80),
        "accent and labels must render"
    );
    if let Some(path) = std::env::var_os("WINARCHY_WORKSPACE_RENDER") {
        let image = image::RgbaImage::from_fn(w as u32, h as u32, |x, y| {
            let p = pixels[y as usize * w + x as usize];
            image::Rgba([p.red, p.green, p.blue, p.alpha])
        });
        image.save(path).unwrap();
    }
    switcher.input(6, Input::Select(9));
    assert_eq!(switcher.selected(), Some(5), "stale input ignored");
    for key in ["h", "k", "j", "l"] {
        switcher.input(7, Input::Key(key.into()));
    }
    assert_eq!(switcher.selected(), Some(5));
    switcher.input(7, Input::Key("9".into()));
    assert_eq!(switcher.selected(), Some(9));
    assert_eq!(
        switcher.scene.as_ref().unwrap().active,
        5,
        "browsing must not switch"
    );
    assert_eq!(switcher.closing, Outcome::None);
    switcher.model.advance(crate::workspace_switcher::DURATION);
    switcher.render();
    switcher.remove(2);
    assert_eq!(switcher.rows.row_data(4).unwrap().detail, "2 windows");
    switcher.input(
        7,
        Input::Key(char::from(slint::platform::Key::Return).to_string()),
    );
    switcher.input(7, Input::Select(2));
    assert_eq!(
        switcher.closing,
        Outcome::Activate(9),
        "confirmation target frozen during fade"
    );
    switcher.last = Instant::now() - Duration::from_millis(200);
    assert_eq!(switcher.poll(), Outcome::Activate(9));
    assert_eq!(switcher.close(), Some(123));
    assert!(!switcher.opened);
    assert_eq!(switcher.rows.row_count(), 0);
    assert_eq!(switcher.placeholders.row_count(), 0);
    assert_eq!(switcher.selected(), None);
    assert_eq!(switcher.close(), None);
    assert_eq!(switcher.poll(), Outcome::None);
    switcher.input(7, Input::Select(1));
    assert_eq!(switcher.selected(), None);
    // Cancellation keeps the original active workspace and returns its focus.
    switcher.opened = true;
    switcher.closing = Outcome::None;
    switcher.restore = Some(456);
    let epoch = switcher.epoch.get();
    switcher.input(
        epoch,
        Input::Key(char::from(slint::platform::Key::Escape).to_string()),
    );
    assert_eq!(switcher.closing, Outcome::Close);
    assert_eq!(switcher.close(), Some(456));
}
