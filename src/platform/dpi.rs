use crate::layout::Rect;
use windows::Win32::{Foundation::POINT, Graphics::Gdi::*, UI::HiDpi::*};
/// Configuration dimensions are logical pixels; layout and HWND operations use physical pixels.
pub fn scale(r: Rect, logical: i32) -> i32 {
    ((i64::from(logical) * i64::from(dpi(r)) + 48) / 96) as i32
}
/// Slint sizes its windows in logical pixels and only learns the scale factor
/// once the native window exists, so surfaces are sized through this instead.
pub fn logical(r: Rect, physical: i32) -> f32 {
    physical as f32 * 96.0 / dpi(r) as f32
}
fn dpi(r: Rect) -> u32 {
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
        x
    }
}
