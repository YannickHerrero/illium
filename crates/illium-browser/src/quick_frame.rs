//! Owned, non-activating Slint scrim around a quick view. The hole over the
//! quick WebView is hit-test transparent so pointer input reaches the page.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::surface::{Surface, place_popup};
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::*,
};

thread_local! { static HOLE: RefCell<RECT> = const { RefCell::new(RECT { left: 0, top: 0, right: 0, bottom: 0 }) }; }

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            crate::native::paint_quick_frame();
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            let mut point = POINT {
                x: (lp.0 & 0xffff) as i16 as i32,
                y: ((lp.0 >> 16) & 0xffff) as i16 as i32,
            };
            let _ = ScreenToClient(hwnd, &mut point);
            let hole = HOLE.with(|h| *h.borrow());
            if PtInRect(&hole, point).as_bool() {
                LRESULT(HTTRANSPARENT as isize)
            } else {
                LRESULT(HTCLIENT as isize)
            }
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            crate::native::quick_scrim_clicked();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub struct QuickFrame {
    pub hwnd: HWND,
    surface: RefCell<Surface>,
}
impl QuickFrame {
    pub unsafe fn new(parent: HWND, instance: HINSTANCE) -> Result<Self> {
        let class = w!("IlliumQuickFrame");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            class,
            w!("Quick link"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            None,
        )?;
        let surface = Surface::new(hwnd, 3)?;
        let frame = Self {
            hwnd,
            surface: RefCell::new(surface),
        };
        frame.set_theme(&illium_theme::Theme::current(&illium_theme::config_home()));
        Ok(frame)
    }
    pub unsafe fn set_theme(&self, theme: &illium_theme::Theme) {
        self.surface.borrow().theme(theme);
        let _ = InvalidateRect(Some(self.hwnd), None, false);
    }
    /// The hole, in client pixels of `parent`, where the quick WebView sits.
    pub unsafe fn hole(parent: HWND) -> RECT {
        let mut rect = RECT::default();
        let _ = GetClientRect(parent, &mut rect);
        let scale = GetDpiForWindow(parent) as i32;
        let margin_x = (rect.right * 6 / 100).max(24 * scale / 96);
        let margin_y = (rect.bottom * 6 / 100).max(24 * scale / 96);
        RECT {
            left: margin_x,
            top: margin_y,
            right: (rect.right - margin_x).max(margin_x + 1),
            bottom: (rect.bottom - margin_y).max(margin_y + 1),
        }
    }
    pub unsafe fn layout(&self, parent: HWND, visible: bool) {
        if !visible {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            return;
        }
        let mut rect = RECT::default();
        let _ = GetClientRect(parent, &mut rect);
        let hole = Self::hole(parent);
        HOLE.with(|h| *h.borrow_mut() = hole);
        let scale = GetDpiForWindow(parent) as f32 / 96.;
        {
            // Released before showing: a synchronous WM_PAINT borrows it mutably.
            let ui = &self.surface.borrow().ui;
            ui.set_hole_x(hole.left as f32 / scale);
            ui.set_hole_y(hole.top as f32 / scale);
            ui.set_hole_width((hole.right - hole.left) as f32 / scale);
            ui.set_hole_height((hole.bottom - hole.top) as f32 / scale);
        }
        place_popup(self.hwnd, parent, 0, 0, rect.right, rect.bottom, true);
        let _ = InvalidateRect(Some(self.hwnd), None, false);
    }
    pub unsafe fn paint(&self) {
        if let Err(e) = self.surface.borrow_mut().paint() {
            eprintln!("Quick frame composition failed: {e}");
        }
    }
}
