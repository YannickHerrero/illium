//! Blur behind a window through the undocumented accent policy of
//! `SetWindowCompositionAttribute`. The documented DWM backdrops (Acrylic,
//! Mica) only render on the active window, which a tiling layout never has
//! alone. It only takes effect where the window's own pixels are translucent.
use std::ffi::c_void;
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
    data: *mut c_void,
    size: usize,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}
const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
/// `hwnd` must be a window of the calling process.
pub fn set(hwnd: isize, enabled: bool) {
    let module: Vec<u16> = "user32.dll\0".encode_utf16().collect();
    unsafe {
        let user32 = GetModuleHandleW(module.as_ptr());
        if user32.is_null() {
            return;
        }
        let address = GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr().cast());
        if address.is_null() {
            return;
        }
        let apply: unsafe extern "system" fn(isize, *mut CompositionData) -> i32 =
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
        apply(hwnd, &mut data);
    }
}
