use crate::layout::Rect;
use windows::Win32::{Foundation::POINT, Graphics::Gdi::*, UI::HiDpi::*};
/// Configuration dimensions are logical pixels; layout and HWND operations use physical pixels.
pub fn scale(r: Rect, logical: i32) -> i32 {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: r.x + r.w / 2,
                y: r.y + r.h / 2,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let (mut x, mut y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        ((i64::from(logical) * i64::from(x) + 48) / 96) as i32
    }
}
