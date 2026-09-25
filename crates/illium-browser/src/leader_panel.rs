//! Owned, non-activating Slint leader surface. Text is opaque; only its background
//! is translucent. Its composition/blur never changes the underlying web page.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    native::wide,
    surface::{PaletteRow, Surface, place_popup},
};
use illium_browser::leader::{Menu, Target};
use slint::{ModelRc, VecModel};
use std::{cell::RefCell, time::Instant};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::*,
};
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            crate::native::paint_leader();
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub struct LeaderPanel {
    pub hwnd: HWND,
    surface: RefCell<Surface>,
    pub notice: Option<(String, Instant)>,
}
impl LeaderPanel {
    pub unsafe fn new(parent: HWND, instance: HINSTANCE) -> Result<Self> {
        let class = w!("IlliumLeaderPanel");
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
            w!("Leader"),
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
        let surface = Surface::new(hwnd, 2)?;
        let mut panel = Self {
            hwnd,
            surface: RefCell::new(surface),
            notice: None,
        };
        panel.set_theme(&illium_theme::Theme::current(&illium_theme::config_home()));
        Ok(panel)
    }
    pub unsafe fn set_font(&mut self, _: HWND) {} // Slint follows the surface DPI.
    pub unsafe fn set_theme(&mut self, theme: &illium_theme::Theme) {
        self.surface.borrow().theme(theme);
        illium_theme::blur::set(self.hwnd.0 as isize, theme.background_blur);
        let _ = InvalidateRect(Some(self.hwnd), None, false);
    }
    pub unsafe fn layout(&self, parent: HWND, menu: Option<Menu>) {
        if menu.is_none() && self.notice.is_none() {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            return;
        }
        let title = format!(
            "Leader — {}",
            menu.map(Menu::title).unwrap_or("Information")
        );
        let _ = SetWindowTextW(self.hwnd, PCWSTR(wide(&title).as_ptr()));
        let mut rect = RECT::default();
        let _ = GetClientRect(parent, &mut rect);
        let scale = GetDpiForWindow(parent) as i32;
        let px = |n: i32| n * scale / 96;
        let margin = px(12);
        let width = px(if menu == Some(Menu::Root) { 576 } else { 384 })
            .min((rect.right - 2 * margin).max(1));
        let columns = if width >= px(544) { 2 } else { 1 };
        self.surface.borrow().ui.set_columns(columns);
        let count = menu.map(|m| m.entries().len() as i32).unwrap_or(1);
        let rows = (count + columns - 1) / columns;
        let height = px(100 + rows * 32).min((rect.bottom - 2 * margin).max(1));
        place_popup(
            self.hwnd,
            parent,
            (rect.right - width - margin).max(0),
            (rect.bottom - height - margin).max(0),
            width,
            height,
            true,
        );
        let _ = InvalidateRect(Some(self.hwnd), None, false);
    }
    #[cfg(test)]
    pub unsafe fn assert_alpha(&self, background: u8) {
        self.surface.borrow_mut().assert_alpha(background, true);
    }
    pub unsafe fn paint(&self, menu: Option<Menu>) {
        let mut surface = self.surface.borrow_mut();
        surface.ui.set_heading(
            format!(
                "LEADER › {}",
                menu.map(Menu::title).unwrap_or("Information")
            )
            .into(),
        );
        surface.ui.set_row_height(32.);
        let rows = if let Some(menu) = menu {
            menu.entries()
                .iter()
                .map(|entry| PaletteRow {
                    title: format!(
                        "{}  →  {}{}",
                        entry.key,
                        entry.label,
                        if matches!(entry.target, Target::Menu(_)) {
                            "  ›"
                        } else {
                            ""
                        }
                    )
                    .into(),
                    ..Default::default()
                })
                .collect()
        } else {
            self.notice
                .iter()
                .map(|(notice, _)| PaletteRow {
                    title: notice.as_str().into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>()
        };
        surface.ui.set_rows(ModelRc::new(VecModel::from(rows)));
        surface.ui.set_footer(
            menu.map(|m| format!("Ctrl+B {} · ⌫ back · Esc close", m.prefix()))
                .unwrap_or_default()
                .into(),
        );
        if let Err(e) = surface.paint() {
            eprintln!("Leader composition failed: {e}");
        }
    }
}
