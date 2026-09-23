//! Blur behind a window through the undocumented accent policy of
//! `SetWindowCompositionAttribute`. The documented DWM backdrops (Acrylic,
//! Mica) only render on the active window, which a tiling layout never has
//! alone.
use windows::{
    Win32::{
        Foundation::HWND,
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    },
    core::{s, w},
};
#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient: u32,
    animation: u32,
}
#[repr(C)]
struct CompositionData {
    attribute: u32,
    data: *mut core::ffi::c_void,
    size: usize,
}
const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
pub fn set(hwnd: HWND, enabled: bool) {
    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else {
            return;
        };
        let Some(address) = GetProcAddress(user32, s!("SetWindowCompositionAttribute")) else {
            return;
        };
        let apply: unsafe extern "system" fn(HWND, *mut CompositionData) -> i32 =
            std::mem::transmute(address);
        let mut policy = AccentPolicy {
            state: if enabled {
                ACCENT_ENABLE_BLURBEHIND
            } else {
                ACCENT_DISABLED
            },
            flags: 0,
            gradient: 0,
            animation: 0,
        };
        let mut data = CompositionData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut _ as *mut _,
            size: size_of::<AccentPolicy>(),
        };
        let _ = apply(hwnd, &mut data);
    }
}
