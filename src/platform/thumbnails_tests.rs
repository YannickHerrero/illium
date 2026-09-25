//! Offscreen DWM smoke test: no daemon, hooks or changes to managed windows.
use super::*;
use windows::{
    Win32::{Foundation::HWND, UI::WindowsAndMessaging::*},
    core::w,
};

struct Offscreen(HWND);
impl Offscreen {
    fn new(x: i32) -> Self {
        Self(unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Illium thumbnail test"),
                WS_POPUP | WS_VISIBLE,
                x,
                -10000,
                640,
                480,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        })
    }
    fn id(&self) -> isize {
        self.0.0 as isize
    }
}
impl Drop for Offscreen {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

#[test]
#[ignore = "requires Windows DWM; offscreen windows only; run alone"]
fn native_workspace_thumbnail_fragments_reuse_and_release_registrations() {
    let foreground = unsafe { GetForegroundWindow() };
    let source = Offscreen::new(-32000);
    let destination = Offscreen::new(-10000);
    let mut thumbnails = Thumbnails::new(destination.id());
    let region = Rect {
        x: 0,
        y: 0,
        w: 320,
        h: 480,
    };
    let dest = Rect {
        x: 0,
        y: 0,
        w: 160,
        h: 240,
    };
    assert!(thumbnails.place_part(source.id(), 0, dest, Some(region), 255));
    assert!(thumbnails.place_part(
        source.id(),
        1,
        Rect { x: 160, ..dest },
        Some(Rect { x: 320, ..region }),
        180
    ));
    assert_eq!(thumbnails.handles.len(), 2);
    let handles = thumbnails.handles.clone();
    for step in 0..30 {
        assert!(thumbnails.place_part(source.id(), 0, Rect { x: step, ..dest }, Some(region), 200));
    }
    assert_eq!(
        thumbnails.handles, handles,
        "animation must reuse registrations"
    );
    for handle in handles.values() {
        let size = unsafe { DwmQueryThumbnailSourceSize(*handle) }.unwrap();
        // This test executable is not per-monitor-DPI-aware like the daemon:
        // DWM returns physical dimensions while CreateWindow takes virtual ones.
        assert!(size.cx >= 640 && size.cy >= 480);
        assert_eq!(size.cx * 3, size.cy * 4);
    }
    thumbnails.retain_parts(&[(source.id(), 0)]);
    assert_eq!(thumbnails.handles.len(), 1);
    assert!(unsafe { DwmQueryThumbnailSourceSize(handles[&(source.id(), 1)]) }.is_err());
    drop(thumbnails);
    assert!(unsafe { DwmQueryThumbnailSourceSize(handles[&(source.id(), 0)]) }.is_err());
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
}
