//! Exposé thumbnails and application icons, produced on a worker thread.
//! PrintWindow makes the target repaint synchronously, so a hung application
//! must never stall the UI thread; results come back through the event queue.
use super::{Event, EventSender, native, shell::expose::Input};
use std::sync::atomic::{AtomicU64, Ordering};
use windows::{
    Win32::{
        Graphics::Gdi::*,
        Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};
/// Straight-alpha RGBA pixels, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
/// Opening generation of the exposé being served; an older worker stops
/// between windows once a newer opening supersedes it.
static CURRENT: AtomicU64 = AtomicU64::new(0);
pub fn cancel() {
    CURRENT.store(0, Ordering::Relaxed);
}
/// Captures every window of `jobs` at most `max_width` physical pixels wide.
/// Icons come first, one per distinct executable, so cards get their badge
/// before the slower window captures arrive.
pub fn start(epoch: u64, jobs: Vec<(isize, String)>, max_width: i32, tx: EventSender) {
    CURRENT.store(epoch, Ordering::Relaxed);
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut exes: Vec<&str> = jobs.iter().map(|(_, exe)| exe.as_str()).collect();
        exes.sort_unstable();
        exes.dedup();
        for exe in exes {
            if CURRENT.load(Ordering::Relaxed) != epoch {
                return;
            }
            if let Some(pixels) = icon(exe) {
                let _ = tx.send(Event::Expose(epoch, Input::Icon(exe.to_owned(), pixels)));
            }
        }
        let mut captured = 0;
        for (id, _) in &jobs {
            if CURRENT.load(Ordering::Relaxed) != epoch {
                return;
            }
            let pixels = window(*id, max_width);
            captured += usize::from(pixels.is_some());
            let _ = tx.send(Event::Expose(epoch, Input::Thumbnail(*id, pixels)));
        }
        tracing::debug!(
            windows = jobs.len(),
            captured,
            elapsed_ms = started.elapsed().as_millis(),
            "exposé thumbnails captured"
        );
    });
}
/// 32-bit top-down DIB selected into its own memory DC.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    width: i32,
    height: i32,
}
impl Surface {
    fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
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
                width,
                height,
            })
        }
    }
    /// BGRA rows out of the section, converted to RGBA. `opaque` forces full
    /// alpha: window captures carry garbage in that channel.
    fn pixels(&self, opaque: bool) -> Pixels {
        let _ = unsafe { GdiFlush() };
        let len = (self.width * self.height * 4) as usize;
        let bgra = unsafe { std::slice::from_raw_parts(self.bits, len) };
        let mut rgba = Vec::with_capacity(len);
        for px in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], if opaque { 255 } else { px[3] }]);
        }
        Pixels {
            width: self.width as u32,
            height: self.height as u32,
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
/// The window's visible frame, scaled down to `max_width`, or None when it
/// cannot be captured (gone, minimized, or painted as a uniform block).
fn window(id: isize, max_width: i32) -> Option<Pixels> {
    let h = native::hwnd(id);
    unsafe {
        if !IsWindow(Some(h)).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
    }
    let window = native::rect(id);
    let frame = native::frame(id);
    let full = Surface::new(window.w, window.h)?;
    // Redirection surfaces of DirectComposition windows (browsers, Electron,
    // WinUI) are only reachable with the full-content flag.
    if unsafe { !PrintWindow(h, full.dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool() } {
        return None;
    }
    let scale = (max_width as f32 / frame.w.max(1) as f32).min(1.0);
    let thumb = Surface::new(
        ((frame.w as f32 * scale) as i32).max(1),
        ((frame.h as f32 * scale) as i32).max(1),
    )?;
    unsafe {
        SetStretchBltMode(thumb.dc, HALFTONE);
        if !StretchBlt(
            thumb.dc,
            0,
            0,
            thumb.width,
            thumb.height,
            Some(full.dc),
            frame.x - window.x,
            frame.y - window.y,
            frame.w,
            frame.h,
            SRCCOPY,
        )
        .as_bool()
        {
            return None;
        }
    }
    let pixels = thumb.pixels(true);
    // A window that did not paint (some UWP hosts, windows on a locked
    // surface) comes back as one flat color; the card falls back to its icon.
    let uniform = pixels
        .rgba
        .chunks_exact(4)
        .all(|px| px[..3] == pixels.rgba[..3]);
    (!uniform).then_some(pixels)
}
const ICON: i32 = 32;
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
        let pixels = surface.filter(|_| drawn)?.pixels(false);
        // Legacy icons without an alpha channel draw fully transparent here.
        let transparent = pixels.rgba.chunks_exact(4).all(|px| px[3] == 0);
        (!transparent).then_some(pixels)
    }
}
