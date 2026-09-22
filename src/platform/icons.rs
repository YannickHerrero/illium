//! Application icons for the exposé cards, drawn on a worker thread: the shell
//! icon lookup touches the executable on disk and must not stall the UI.
use super::{Event, EventSender, native, shell::expose::Input};
use std::sync::atomic::{AtomicU64, Ordering};
use windows::{
    Win32::{Graphics::Gdi::*, UI::Shell::*, UI::WindowsAndMessaging::*},
    core::PCWSTR,
};
/// Premultiplied RGBA pixels, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
/// Opening generation of the exposé being served; an older worker stops
/// once a newer opening supersedes it.
static CURRENT: AtomicU64 = AtomicU64::new(0);
pub fn cancel() {
    CURRENT.store(0, Ordering::Relaxed);
}
/// Looks one icon up per executable path.
pub fn start(epoch: u64, mut exes: Vec<String>, tx: EventSender) {
    CURRENT.store(epoch, Ordering::Relaxed);
    exes.sort_unstable();
    exes.dedup();
    std::thread::spawn(move || {
        for exe in exes {
            if CURRENT.load(Ordering::Relaxed) != epoch {
                return;
            }
            if let Some(pixels) = icon(&exe) {
                let _ = tx.send(Event::Expose(epoch, Input::Icon(exe, pixels)));
            }
        }
    });
}
const ICON: i32 = 32;
/// 32-bit top-down DIB selected into its own memory DC, zero-filled.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
}
impl Surface {
    fn new(width: i32, height: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(dc);
                return None;
            };
            SelectObject(dc, bitmap.into());
            Some(Self {
                dc,
                bitmap,
                bits: bits.cast(),
            })
        }
    }
    fn pixels(&self) -> Pixels {
        let _ = unsafe { GdiFlush() };
        let len = (ICON * ICON * 4) as usize;
        let bgra = unsafe { std::slice::from_raw_parts(self.bits, len) };
        let mut rgba = Vec::with_capacity(len);
        for px in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        Pixels {
            width: ICON as u32,
            height: ICON as u32,
            rgba,
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}
/// The executable's large shell icon, alpha preserved.
fn icon(exe: &str) -> Option<Pixels> {
    if exe.is_empty() {
        return None;
    }
    let path = native::wide(exe);
    let mut info = SHFILEINFOW::default();
    unsafe {
        if SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        ) == 0
            || info.hIcon.is_invalid()
        {
            return None;
        }
        let surface = Surface::new(ICON, ICON);
        let drawn = surface.as_ref().is_some_and(|s| {
            DrawIconEx(s.dc, 0, 0, info.hIcon, ICON, ICON, 0, None, DI_NORMAL).is_ok()
        });
        let _ = DestroyIcon(info.hIcon);
        let pixels = surface.filter(|_| drawn)?.pixels();
        // Legacy icons without an alpha channel draw fully transparent here.
        let transparent = pixels.rgba.chunks_exact(4).all(|px| px[3] == 0);
        (!transparent).then_some(pixels)
    }
}
