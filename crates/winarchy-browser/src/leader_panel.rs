//! Non-activating native leader overlay; never takes focus from the page/editor.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::native::wide;
use std::time::Instant;
use winarchy_browser::leader::{Leader, Menu, Target};
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
            let dc = BeginPaint(hwnd, &mut ps);
            crate::native::paint_leader(dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
fn color(value: &str) -> COLORREF {
    let (r, g, b) = winarchy_theme::rgb(value).unwrap();
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
unsafe fn fill(dc: HDC, rect: RECT, value: &str) {
    let brush = CreateSolidBrush(color(value));
    FillRect(dc, &rect, brush);
    let _ = DeleteObject(brush.into());
}
unsafe fn text(dc: HDC, mut rect: RECT, value: &str, tint: &str) {
    // An empty UTF-16 Vec does not provide a readable string pointer to User32.
    if value.is_empty() {
        return;
    }
    SetTextColor(dc, color(tint));
    let mut value: Vec<u16> = value.encode_utf16().collect();
    DrawTextW(
        dc,
        &mut value,
        &mut rect,
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
}
pub struct LeaderPanel {
    pub hwnd: HWND,
    theme: winarchy_theme::Theme,
    font: HFONT,
    pub notice: Option<(String, Instant)>,
}
impl LeaderPanel {
    pub unsafe fn new(parent: HWND, instance: HINSTANCE) -> Result<Self> {
        let class = w!("WinarchyLeaderPanel");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE,
            class,
            w!("Leader"),
            WS_CHILD,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            None,
        )?;
        let mut panel = Self {
            hwnd,
            theme: winarchy_theme::Theme::current(&winarchy_theme::config_home()),
            font: HFONT::default(),
            notice: None,
        };
        panel.set_font(parent);
        Ok(panel)
    }
    fn px(&self, value: i32) -> i32 {
        // Match the compact navigation palette's spacing and DPI scaling.
        unsafe { value * GetDpiForWindow(self.hwnd) as i32 * 4 / (96 * 5) }
    }
    pub unsafe fn set_font(&mut self, parent: HWND) {
        let font = CreateFontW(
            -(13 * GetDpiForWindow(parent) as i32 / 96),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(wide("Cascadia Mono").as_ptr()),
        );
        if !font.is_invalid() {
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
            self.font = font;
        }
    }
    pub unsafe fn set_theme(&mut self, theme: &winarchy_theme::Theme) {
        self.theme = theme.clone();
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
        let margin = self.px(15); // 12 logical pixels from the client edges.
        let preferred_width = if menu == Some(Menu::Root) { 720 } else { 480 };
        let width = self
            .px(preferred_width)
            .min((rect.right - 2 * margin).max(1));
        let columns = if width >= self.px(680) { 2 } else { 1 };
        let count = menu.map(|m| m.entries().len() as i32).unwrap_or(1);
        let rows = (count + columns - 1) / columns;
        let height = (self.px(112) + rows * self.px(40)).min((rect.bottom - 2 * margin).max(1));
        let _ = SetWindowPos(
            self.hwnd,
            Some(HWND_TOP),
            (rect.right - width - margin).max(0),
            (rect.bottom - height - margin).max(0),
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(Some(self.hwnd), None, false);
    }
    pub unsafe fn paint(&self, dc: HDC, leader: &Leader) {
        let mut r = RECT::default();
        let _ = GetClientRect(self.hwnd, &mut r);
        let t = &self.theme;
        fill(dc, r, &t.surface);
        let border = CreateSolidBrush(color(&t.overlay));
        FrameRect(dc, &r, border);
        let _ = DeleteObject(border.into());
        let previous = SelectObject(dc, self.font.into());
        SetBkMode(dc, TRANSPARENT);
        let p = |v| self.px(v);
        let box_at = |x, y, w, h| RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let menu = leader.menu();
        text(dc, box_at(p(24), p(12), p(96), p(40)), "LEADER", &t.accent);
        text(
            dc,
            box_at(p(130), p(12), r.right - p(260), p(40)),
            &format!("› {}", menu.map(Menu::title).unwrap_or("Information")),
            &t.text,
        );
        if let Some(menu) = menu {
            text(
                dc,
                box_at(r.right - p(122), p(12), p(100), p(40)),
                &format!("{} touches", menu.entries().len()),
                &t.subtext,
            );
            let columns = if r.right >= p(680) { 2 } else { 1 };
            let rows = (menu.entries().len() as i32 + columns - 1) / columns;
            let cell_width = (r.right - p(40)) / columns;
            let row_height = ((r.bottom - p(112)) / rows).min(p(40)).max(1);
            for (i, entry) in menu.entries().iter().enumerate() {
                let i = i as i32;
                let x = p(20) + (i % columns) * cell_width;
                let y = p(64) + (i / columns) * row_height;
                if matches!(entry.target, Target::Menu(_)) {
                    fill(
                        dc,
                        box_at(x, y, cell_width - p(12), row_height - p(4)),
                        &t.background,
                    );
                }
                let key_rect = box_at(x + p(8), y + p(6), p(30), (row_height - p(12)).max(1));
                let brush = CreateSolidBrush(color(&t.overlay));
                FrameRect(dc, &key_rect, brush);
                let _ = DeleteObject(brush.into());
                text(
                    dc,
                    box_at(x + p(15), y, p(24), row_height),
                    &entry.key.to_string(),
                    &t.accent,
                );
                text(dc, box_at(x + p(48), y, p(22), row_height), "→", &t.subtext);
                text(
                    dc,
                    box_at(x + p(76), y, cell_width - p(112), row_height),
                    entry.label,
                    &t.text,
                );
                if matches!(entry.target, Target::Menu(_)) {
                    text(
                        dc,
                        box_at(x + cell_width - p(32), y, p(20), row_height),
                        "›",
                        &t.subtext,
                    );
                }
            }
            let footer = format!("Ctrl+B {}", menu.prefix());
            text(
                dc,
                box_at(p(24), r.bottom - p(46), r.right / 2 - p(24), p(40)),
                &footer,
                &t.subtext,
            );
            let help = if r.right >= p(680) {
                "⌫ retour · Échap fermer"
            } else {
                "⌫ retour · Esc"
            };
            text(
                dc,
                box_at(r.right / 2, r.bottom - p(46), r.right / 2 - p(16), p(40)),
                help,
                &t.subtext,
            );
        } else if let Some((notice, _)) = &self.notice {
            text(
                dc,
                box_at(p(24), p(64), r.right - p(48), p(36)),
                notice,
                &t.text,
            );
        }
        fill(dc, box_at(1, p(60), r.right - 2, 1), &t.overlay);
        fill(dc, box_at(1, r.bottom - p(50), r.right - 2, 1), &t.overlay);
        SelectObject(dc, previous);
    }
}
impl Drop for LeaderPanel {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
        }
    }
}
