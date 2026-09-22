//! Opt-in native smoke: offscreen surfaces only, no hooks or session daemon.
use super::*;
use slint::winit_030::{SlintEvent, winit};
use winit::platform::windows::EventLoopBuilderExtWindows;

#[test]
#[ignore = "requires the Windows compositor; offscreen windows only; run alone"]
fn native_prewarm_retains_hwnd_without_stealing_focus() {
    let mut builder = winit::event_loop::EventLoop::<SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    super::super::backend()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        let popup = Popup::new().unwrap();
        prewarm(&popup);
        // A user opening wins over the scheduled completion of prewarming.
        let opened = Popup::new().unwrap();
        prewarm(&opened);
        prepare(
            opened.window(),
            Rect {
                x: -8000,
                y: -8000,
                w: 300,
                h: 160,
            },
            true,
        );
        opened.show().unwrap();
        slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
            let hwnd = id(popup.window());
            assert_ne!(hwnd, 0);
            assert!(!native::visible(hwnd));
            assert!(
                native::visible(id(opened.window())),
                "prewarm must not hide a real opening"
            );
            opened.hide().unwrap();
            assert_eq!(unsafe { GetForegroundWindow() }, foreground);
            let rect = Rect {
                x: -10000,
                y: -10000,
                w: 400,
                h: 240,
            };
            popup.set_surface_width(super::super::dpi::logical(rect, rect.w));
            popup.set_surface_height(super::super::dpi::logical(rect, rect.h));
            prepare(popup.window(), rect, true);
            popup.show().unwrap();
            slint::Timer::single_shot(std::time::Duration::from_millis(40), move || {
                assert_eq!(
                    id(popup.window()),
                    hwnd,
                    "show must reuse the prepared HWND"
                );
                assert_eq!(
                    native::rect(hwnd),
                    rect,
                    "first show must use final physical geometry"
                );
                assert_eq!(unsafe { GetForegroundWindow() }, foreground);
                popup.hide().unwrap();
                assert!(!native::visible(hwnd));
                slint::quit_event_loop().unwrap();
            });
        });
    });
    slint::run_event_loop_until_quit().unwrap();
}
