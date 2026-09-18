//! Native navigation palette. One EDIT and an owner-drawn local result list.
#![allow(unsafe_op_in_unsafe_fn)]
use super::native::wide;
use std::{cell::RefCell, rc::Rc};
use winarchy_browser::library::{Library, Suggestion};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};
pub const EDIT_ID: usize = 101;
pub const LIST_ID: usize = 102;
pub const PANEL_ID: usize = 104;

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND | WM_DRAWITEM | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            SendMessageW(GetParent(hwnd).unwrap_or_default(), msg, Some(wp), Some(lp))
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            super::native::paint_picker(dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
fn color(s: &str) -> COLORREF {
    let (r, g, b) = winarchy_theme::rgb(s).unwrap();
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
unsafe fn fill(dc: HDC, rect: &RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, rect, brush);
    let _ = DeleteObject(brush.into());
}
unsafe fn text(dc: HDC, rect: RECT, value: &str, color: COLORREF) {
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT);
    let mut value: Vec<u16> = value.encode_utf16().collect();
    let mut rect = rect;
    DrawTextW(
        dc,
        &mut value,
        &mut rect,
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
}
pub struct Picker {
    pub panel: HWND,
    pub edit: HWND,
    pub list: HWND,
    font: HFONT,
    pub visible: bool,
    pub home: bool,
    pub library: Rc<RefCell<Library>>,
    suggestions: Vec<Suggestion>,
    theme: winarchy_theme::Theme,
    status: RefCell<String>,
}
impl Picker {
    pub unsafe fn new(
        parent: HWND,
        instance: HINSTANCE,
        library: Rc<RefCell<Library>>,
    ) -> Result<Self> {
        let class = w!("WinarchyNavigationPalette");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(panel_proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let panel = CreateWindowExW(
            Default::default(),
            class,
            w!("Navigation"),
            WS_CHILD | WS_CLIPCHILDREN,
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(PANEL_ID as *mut _)),
            Some(instance),
            None,
        )?;
        let child = |class, style, id| {
            CreateWindowExW(
                Default::default(),
                class,
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
                0,
                0,
                1,
                1,
                Some(panel),
                Some(HMENU(id as *mut _)),
                Some(instance),
                None,
            )
        };
        let edit = child(w!("EDIT"), WINDOW_STYLE(ES_AUTOHSCROLL as u32), EDIT_ID)?;
        let list = child(
            w!("LISTBOX"),
            WS_VSCROLL
                | WINDOW_STYLE(
                    (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS)
                        as u32,
                ),
            LIST_ID,
        )?;
        let mut picker = Self {
            panel,
            edit,
            list,
            font: HFONT::default(),
            visible: false,
            home: false,
            library,
            suggestions: Vec::new(),
            theme: winarchy_theme::Theme::current(&winarchy_theme::config_home()),
            status: RefCell::new(String::new()),
        };
        picker.set_font(parent);
        SendMessageW(
            edit,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(w!("Rechercher ou saisir une adresse…").0 as isize)),
        );
        SendMessageW(edit, 0x00C5, Some(WPARAM(4096)), None);
        Ok(picker)
    }
    pub unsafe fn set_theme(&mut self, theme: &winarchy_theme::Theme) {
        self.theme = theme.clone();
        let _ = RedrawWindow(
            Some(self.panel),
            None,
            None,
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
    fn px(&self, n: i32) -> i32 {
        unsafe { (n * GetDpiForWindow(self.panel) as i32 / 96).max(1) }
    }
    pub unsafe fn set_font(&mut self, parent: HWND) {
        let font = CreateFontW(
            -(16 * GetDpiForWindow(parent) as i32 / 96),
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
            w!("Cascadia Mono"),
        );
        if font.is_invalid() {
            return;
        }
        for hwnd in [self.edit, self.list] {
            SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        if !self.font.is_invalid() {
            let _ = DeleteObject(self.font.into());
        }
        self.font = font;
    }
    pub unsafe fn text(&self) -> String {
        let mut text = vec![0; GetWindowTextLengthW(self.edit) as usize + 1];
        let len = GetWindowTextW(self.edit, &mut text);
        String::from_utf16_lossy(&text[..len as usize])
    }
    pub unsafe fn show(&mut self, parent: HWND, home: bool, current: &str) {
        self.visible = true;
        self.home = home;
        if let Err(e) = self.library.borrow_mut().reload() {
            eprintln!("Cannot reload history: {e}");
        }
        let _ = SetWindowTextW(self.edit, PCWSTR(wide(current).as_ptr()));
        self.status.borrow_mut().clear();
        self.refresh(parent);
        let _ = ShowWindow(self.panel, SW_SHOWNA);
        if IsWindowVisible(parent).as_bool() {
            let _ = SetFocus(Some(self.edit));
        }
        SendMessageW(self.edit, 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1)));
    }
    pub unsafe fn hide(&mut self) {
        self.visible = false;
        let _ = ShowWindow(self.panel, SW_HIDE);
    }
    pub unsafe fn status(&self, value: &str) {
        *self.status.borrow_mut() = value.into();
        let _ = InvalidateRect(Some(self.panel), None, false);
    }
    pub unsafe fn refresh(&mut self, parent: HWND) {
        self.suggestions = self.library.borrow().suggestions(&self.text());
        // Preserve fuzzy relevance within each category.
        self.suggestions.sort_by_key(|s| !s.bookmarked);
        self.status.borrow_mut().clear();
        SendMessageW(self.list, WM_SETREDRAW, Some(WPARAM(0)), None);
        SendMessageW(self.list, LB_RESETCONTENT, None, None);
        for s in &self.suggestions {
            let value = wide(&format!("{} — {}", s.site.title, s.site.url));
            SendMessageW(
                self.list,
                LB_ADDSTRING,
                None,
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
        SendMessageW(self.list, WM_SETREDRAW, Some(WPARAM(1)), None);
        self.layout(parent);
        let _ = RedrawWindow(
            Some(self.panel),
            None,
            None,
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
    pub unsafe fn layout(&self, parent: HWND) -> i32 {
        let mut r = RECT::default();
        let _ = GetClientRect(parent, &mut r);
        let p = |n| self.px(n);
        let width = p(850).min((r.right - p(32)).max(1));
        let height =
            p(172 + self.suggestions.len().max(1) as i32 * 88).min((r.bottom - p(32)).max(1));
        let _ = SetWindowPos(
            self.panel,
            Some(HWND_TOP),
            (r.right - width) / 2,
            (r.bottom - height) / 2,
            width,
            height,
            SWP_NOACTIVATE,
        );
        let _ = SetWindowPos(
            self.edit,
            None,
            p(48),
            p(24),
            (width - p(130)).max(1),
            p(28),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(
            self.list,
            LB_SETITEMHEIGHT,
            Some(WPARAM(0)),
            Some(LPARAM(p(88) as isize)),
        );
        let _ = SetWindowPos(
            self.list,
            None,
            p(24),
            p(116),
            (width - p(48)).max(1),
            (height - p(172)).max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = ShowWindow(
            self.list,
            if self.suggestions.is_empty() {
                SW_HIDE
            } else {
                SW_SHOWNA
            },
        );
        0
    }
    pub unsafe fn paint(&self, dc: HDC) {
        let mut r = RECT::default();
        let _ = GetClientRect(self.panel, &mut r);
        let p = |n| self.px(n);
        fill(dc, &r, color(&self.theme.surface));
        let border = CreateSolidBrush(color(&self.theme.overlay));
        FrameRect(dc, &r, border);
        let _ = DeleteObject(border.into());
        let old = SelectObject(dc, self.font.into());
        let line = |y| RECT {
            left: 1,
            top: y,
            right: r.right - 1,
            bottom: y + 1,
        };
        fill(dc, &line(p(76)), color(&self.theme.overlay));
        fill(dc, &line(r.bottom - p(52)), color(&self.theme.overlay));
        let label = |x, y, right, h, value: &str, c: &str| {
            text(
                dc,
                RECT {
                    left: x,
                    top: y,
                    right,
                    bottom: y + h,
                },
                value,
                color(c),
            )
        };
        label(p(18), p(24), p(44), p(28), "⌕", &self.theme.accent);
        label(
            r.right - p(65),
            p(24),
            r.right - p(16),
            p(28),
            "Esc",
            &self.theme.subtext,
        );
        label(
            p(24),
            p(82),
            r.right - p(24),
            p(28),
            "WINARCHY BROWSER › Navigation",
            &self.theme.accent,
        );
        if self.suggestions.is_empty() {
            label(
                p(24),
                p(120),
                r.right - p(24),
                p(40),
                "Aucun résultat local · Entrée pour rechercher",
                &self.theme.subtext,
            );
        }
        let status = self.status.borrow();
        let footer = if status.is_empty() {
            format!(
                "{} résultats · ↑↓ choisir · Entrée ouvrir",
                self.suggestions.len()
            )
        } else {
            status.clone()
        };
        label(
            p(24),
            r.bottom - p(48),
            r.right - p(24),
            p(44),
            &footer,
            &self.theme.subtext,
        );
        SelectObject(dc, old);
    }
    pub unsafe fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        let Some(s) = self.suggestions.get(item.itemID as usize) else {
            return;
        };
        let r = item.rcItem;
        let p = |n| self.px(n);
        let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
        fill(
            item.hDC,
            &r,
            color(if selected {
                &self.theme.overlay
            } else {
                &self.theme.surface
            }),
        );
        let old = SelectObject(item.hDC, self.font.into());
        let category = if s.bookmarked { "FAVORI" } else { "HISTORIQUE" };
        let label = |left, top, right, h, value: &str, c: &str| {
            text(
                item.hDC,
                RECT {
                    left,
                    top,
                    right,
                    bottom: top + h,
                },
                value,
                color(c),
            )
        };
        label(
            r.left + p(12),
            r.top + p(28),
            r.left + p(44),
            p(32),
            if s.bookmarked { "★" } else { "◷" },
            if s.bookmarked {
                &self.theme.yellow
            } else {
                &self.theme.subtext
            },
        );
        if item.itemID == 0 || self.suggestions[item.itemID as usize - 1].bookmarked != s.bookmarked
        {
            label(
                r.left + p(12),
                r.top + p(2),
                r.right - p(140),
                p(22),
                if s.bookmarked {
                    "Favoris"
                } else {
                    "Historique"
                },
                &self.theme.subtext,
            );
        }
        label(
            (r.right - p(130)).max(r.left + p(56)),
            r.top + p(4),
            r.right - p(12),
            p(20),
            category,
            &self.theme.subtext,
        );
        label(
            r.left + p(56),
            r.top + p(28),
            r.right - p(12),
            p(28),
            &s.site.url,
            if selected {
                &self.theme.accent
            } else {
                &self.theme.text
            },
        );
        label(
            r.left + p(56),
            r.top + p(56),
            r.right - p(12),
            p(24),
            &s.description(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default(),
            ),
            &self.theme.subtext,
        );
        SelectObject(item.hDC, old);
    }
    pub unsafe fn choose(&self, direction: i32) {
        let current = SendMessageW(self.list, LB_GETCURSEL, None, None).0 as i32;
        let last = self.suggestions.len() as i32 - 1;
        if last < 0 {
            return;
        }
        let next = if current < 0 {
            if direction > 0 { 0 } else { last }
        } else {
            (current + direction).clamp(0, last)
        };
        SendMessageW(self.list, LB_SETCURSEL, Some(WPARAM(next as usize)), None);
    }
    pub unsafe fn input(&self) -> String {
        let index = SendMessageW(self.list, LB_GETCURSEL, None, None).0;
        if index >= 0
            && let Some(s) = self.suggestions.get(index as usize)
        {
            return s.site.url.clone();
        }
        self.text()
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
        }
    }
}
