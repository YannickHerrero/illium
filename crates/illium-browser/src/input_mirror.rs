//! A visually hidden EDIT remains the sole input/accessibility/IME authority.
//! Print it against black and white, recover coverage, and composite its pixels
//! in Slint. This preserves native shaping, selection and horizontal scrolling
//! without maintaining a second text editor (or using a lossy color key).
#![allow(unsafe_op_in_unsafe_fn)]
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{cell::Cell, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

const STATE: PCWSTR = w!("Illium.InputMirror");
pub(crate) const BLINK_TIMER: usize = 0x494d;
const EM_GETSEL: u32 = 0x00b0;
const EM_SETSEL: u32 = 0x00b1;
const EM_REPLACESEL: u32 = 0x00c2;
const EM_UNDO: u32 = 0x00c7;

struct State {
    previous: WNDPROC,
    // Only set during WM_PRINTCLIENT, never for the control's normal painting.
    background: Cell<Option<bool>>,
    text: Cell<COLORREF>,
    caret: Cell<bool>,
    composing: Cell<bool>,
}

unsafe fn restart_blink(hwnd: HWND, state: &State) {
    state.caret.set(true);
    let _ = KillTimer(Some(hwnd), BLINK_TIMER);
    let interval = GetCaretBlinkTime();
    if GetFocus() == hwnd
        && IsWindowVisible(hwnd).as_bool()
        && interval != 0
        && interval != u32::MAX
    {
        SetTimer(Some(hwnd), BLINK_TIMER, interval, None);
    }
}

unsafe fn invalidate_parent(hwnd: HWND) {
    if let Ok(parent) = GetParent(hwnd) {
        let _ = InvalidateRect(Some(parent), None, false);
    }
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let data = GetPropW(hwnd, STATE).0 as *const State;
    // A native context menu runs a nested message loop. Keep the state alive
    // even if that loop closes the browser and destroys this HWND.
    Rc::increment_strong_count(data);
    let state = Rc::from_raw(data);
    if msg == WM_NCDESTROY {
        let _ = KillTimer(Some(hwnd), BLINK_TIMER);
        SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            state.previous.unwrap() as *const () as isize,
        );
        let _ = RemovePropW(hwnd, STATE);
        let result = CallWindowProcW(state.previous, hwnd, msg, wp, lp);
        drop(Rc::from_raw(data));
        return result;
    }
    if msg == WM_TIMER && wp.0 == BLINK_TIMER {
        if GetFocus() == hwnd && IsWindowVisible(hwnd).as_bool() {
            state.caret.set(!state.caret.get());
            invalidate_parent(hwnd);
        } else {
            let _ = KillTimer(Some(hwnd), BLINK_TIMER);
        }
        return LRESULT(0);
    }
    if msg == WM_IME_STARTCOMPOSITION {
        state.composing.set(true);
    } else if msg == WM_IME_ENDCOMPOSITION {
        state.composing.set(false);
    }
    let result = CallWindowProcW(state.previous, hwnd, msg, wp, lp);
    if !IsWindow(Some(hwnd)).as_bool() {
        return result;
    }
    // Include programmatic changes and native drag/autoscroll, not just keys.
    // Do not invalidate on WM_PRINTCLIENT: painting must not schedule itself.
    if matches!(
        msg,
        WM_SETFOCUS
            | WM_KILLFOCUS
            | WM_KEYDOWN
            | WM_CHAR
            | WM_UNICHAR
            | WM_IME_STARTCOMPOSITION
            | WM_IME_COMPOSITION
            | WM_IME_ENDCOMPOSITION
            | WM_SETTEXT
            | WM_SETFONT
            | WM_SIZE
            | WM_HSCROLL
            | EM_SETSEL
            | EM_REPLACESEL
            | EM_UNDO
            | WM_UNDO
            | WM_CUT
            | WM_PASTE
            | WM_CLEAR
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_LBUTTONDBLCLK
            | WM_CAPTURECHANGED
    ) || (msg == WM_MOUSEMOVE && GetCapture() == hwnd)
        || msg == WM_TIMER
    {
        restart_blink(hwnd, &state);
        invalidate_parent(hwnd);
    }
    result
}

/// Called by the EDIT's parent before the ordinary theme-color handler. No
/// Picker/Surface borrow is needed during synchronous native paint callbacks.
pub unsafe fn control_color(edit: HWND, dc: HDC) -> Option<LRESULT> {
    let data = GetPropW(edit, STATE).0 as *const State;
    let state = data.as_ref()?;
    let white = state.background.get()?;
    SetTextColor(dc, state.text.get());
    SetBkColor(dc, COLORREF(if white { 0xffffff } else { 0 }));
    Some(LRESULT(
        GetStockObject(if white { WHITE_BRUSH } else { BLACK_BRUSH }).0 as isize,
    ))
}

struct Bitmap {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}
impl Bitmap {
    unsafe fn new(width: u32, height: u32) -> Result<Self> {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(Error::from_thread());
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) => bitmap,
            Err(error) => {
                let _ = DeleteDC(dc);
                return Err(error);
            }
        };
        let previous = SelectObject(dc, bitmap.into());
        Ok(Self {
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
            width,
            height,
        })
    }
    unsafe fn pixels(&self) -> &[u8] {
        std::slice::from_raw_parts(self.bits, (self.width * self.height * 4) as usize)
    }
}
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Black = premultiplied foreground; white - black = uncovered background.
/// Unlike chroma-keying, this retains antialiased edges and opaque selections,
/// including text/selection colors that happen to match the palette background.
fn coverage(black: &[u8], white: &[u8]) -> Rgba8Pixel {
    let uncovered = (0..3)
        .map(|i| white[i].saturating_sub(black[i]))
        .min()
        .unwrap();
    Rgba8Pixel::new(black[2], black[1], black[0], 255 - uncovered)
}

pub struct InputMirror {
    edit: HWND,
    state: Rc<State>,
    bitmap: Option<Bitmap>,
    black: Vec<u8>,
}
impl InputMirror {
    pub unsafe fn new(edit: HWND) -> Result<Self> {
        let state = Rc::new(State {
            previous: std::mem::transmute::<isize, WNDPROC>(GetWindowLongPtrW(edit, GWLP_WNDPROC)),
            background: Cell::new(None),
            text: Cell::new(COLORREF(0)),
            caret: Cell::new(true),
            composing: Cell::new(false),
        });
        let data = Rc::into_raw(state.clone());
        if let Err(error) = SetPropW(edit, STATE, Some(HANDLE(data as *mut _))) {
            drop(Rc::from_raw(data));
            return Err(error);
        }
        if SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize) == 0 {
            let error = Error::from_thread();
            let _ = RemovePropW(edit, STATE);
            drop(Rc::from_raw(data));
            return Err(error);
        }
        Ok(Self {
            edit,
            state,
            bitmap: None,
            black: vec![],
        })
    }
    pub fn composing(&self) -> bool {
        self.state.composing.get()
    }
    pub unsafe fn wake(&self) {
        restart_blink(self.edit, &self.state);
        invalidate_parent(self.edit);
    }
    pub unsafe fn sleep(&self) {
        let _ = KillTimer(Some(self.edit), BLINK_TIMER);
    }
    pub unsafe fn image(&mut self, text: COLORREF) -> Result<Image> {
        let mut rect = RECT::default();
        GetClientRect(self.edit, &mut rect)?;
        let size = (rect.right.max(1) as u32, rect.bottom.max(1) as u32);
        if self.bitmap.as_ref().map(|b| (b.width, b.height)) != Some(size) {
            self.bitmap = Some(Bitmap::new(size.0, size.1)?);
        }
        let bitmap = self.bitmap.as_ref().unwrap();
        self.state.text.set(text);
        for white in [false, true] {
            self.state.background.set(Some(white));
            // Initialize even pixels the EDIT might leave untouched.
            FillRect(
                bitmap.dc,
                &rect,
                HBRUSH(GetStockObject(if white { WHITE_BRUSH } else { BLACK_BRUSH }).0),
            );
            // Retain the native cue banner for accessibility, but let Slint
            // draw the empty placeholder in the theme's subtext color. Never
            // hide an in-progress IME composition behind that placeholder.
            if GetWindowTextLengthW(self.edit) != 0 || self.composing() {
                SendMessageW(
                    self.edit,
                    WM_PRINTCLIENT,
                    Some(WPARAM(bitmap.dc.0 as usize)),
                    Some(LPARAM(PRF_CLIENT as isize)),
                );
            }
            let _ = GdiFlush();
            if !white {
                self.black.clear();
                self.black.extend_from_slice(bitmap.pixels());
            }
        }
        self.state.background.set(None);
        let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(size.0, size.1);
        for ((pixel, black), white) in pixels
            .make_mut_slice()
            .iter_mut()
            .zip(self.black.chunks_exact(4))
            .zip(bitmap.pixels().chunks_exact(4))
        {
            *pixel = coverage(black, white);
        }
        // WM_PRINTCLIENT intentionally excludes the system caret. Its native
        // rectangle already includes scrolling, shaping, DPI and IME updates.
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let mut start = 0u32;
        let mut end = 0u32;
        SendMessageW(
            self.edit,
            EM_GETSEL,
            Some(WPARAM(&mut start as *mut _ as usize)),
            Some(LPARAM(&mut end as *mut _ as isize)),
        );
        if self.state.caret.get()
            && GetFocus() == self.edit
            && start == end
            && GetGUIThreadInfo(0, &mut gui).is_ok()
            && gui.hwndCaret == self.edit
        {
            let c = Rgba8Pixel::new(text.0 as u8, (text.0 >> 8) as u8, (text.0 >> 16) as u8, 255);
            for y in gui.rcCaret.top.max(0)..gui.rcCaret.bottom.min(size.1 as i32) {
                for x in gui.rcCaret.left.max(0)..gui.rcCaret.right.min(size.0 as i32) {
                    pixels.make_mut_slice()[(y as u32 * size.0 + x as u32) as usize] = c;
                }
            }
        }
        Ok(Image::from_rgba8_premultiplied(pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recover_transparency_without_erasing_matching_colors() {
        assert_eq!(
            coverage(&[0, 0, 0, 0], &[255, 255, 255, 0]),
            Rgba8Pixel::new(0, 0, 0, 0)
        );
        assert_eq!(
            coverage(&[30, 20, 10, 0], &[30, 20, 10, 0]),
            Rgba8Pixel::new(10, 20, 30, 255)
        );
        assert_eq!(
            coverage(&[15, 10, 5, 0], &[142, 137, 132, 0]),
            Rgba8Pixel::new(5, 10, 15, 128)
        );
        assert_eq!(
            coverage(&[0, 0, 0, 0], &[0, 0, 0, 0]),
            Rgba8Pixel::new(0, 0, 0, 255)
        );
    }
}
