//! Offscreen Win32 regressions. Never enumerate or rearrange user windows.
use super::*;
use windows::core::w;

struct Fixture(isize);
impl Fixture {
    fn new(rect: Rect) -> Self {
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Winarchy offscreen regression"),
                WS_POPUP,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        };
        show(window.0 as isize, true);
        Self(window.0 as isize)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(hwnd(self.0));
        }
    }
}

#[test]
#[ignore = "requires Windows desktop; only offscreen tool windows"]
fn cached_border_reappears_and_repairs_stacking() {
    let frame = Rect {
        x: -10000,
        y: -10000,
        w: 300,
        h: 180,
    };
    let client = Fixture::new(frame);
    let mut border = Border::new().unwrap();
    border.place(client.0, frame, 3, "#cba6f7");
    assert!(visible(border.id));
    assert_eq!(border.shape, Some((306, 186, 3)));
    border.hide();
    let _other = Fixture::new(frame);
    border.place(client.0, frame, 3, "#cba6f7");
    assert!(
        visible(border.id),
        "cached drawing must not skip showing a hidden border"
    );
    assert_eq!(
        unsafe { GetWindow(hwnd(client.0), GW_HWNDPREV).unwrap() },
        hwnd(border.id)
    );
    border.place(client.0, Rect { w: 400, ..frame }, 4, "#ffffff");
    assert_eq!(border.shape, Some((408, 188, 4)));
    assert_eq!(border.color, Some(colorref("#ffffff").0));
}

#[test]
#[ignore = "requires Windows desktop; only offscreen tool windows"]
fn batch_parks_and_restores_without_hiding_clients() {
    let first = Rect {
        x: -10000,
        y: -10000,
        w: 300,
        h: 180,
    };
    let second = Rect {
        x: -9000,
        y: -9000,
        w: 500,
        h: 400,
    };
    let a = Fixture::new(first);
    let b = Fixture::new(parking_rect(second));
    let foreground = unsafe { GetForegroundWindow() };
    batch(&[(a.0, parking_rect(first)), (b.0, second)]);
    assert_eq!(rect(a.0), parking_rect(first));
    assert_eq!(rect(b.0), second);
    assert!(visible(a.0) && visible(b.0));
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    // A destroyed HWND must not prevent surviving clients reaching their tiles.
    let gone = Fixture::new(first);
    let gone_id = gone.0;
    drop(gone);
    batch(&[(a.0, first), (gone_id, first), (b.0, parking_rect(second))]);
    assert_eq!(rect(a.0), first);
    assert_eq!(rect(b.0), parking_rect(second));
}
