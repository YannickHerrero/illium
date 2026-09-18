//! Native navigation palette. One EDIT and an owner-drawn local result list.
#![allow(unsafe_op_in_unsafe_fn)]
use super::native::wide;
use std::{cell::RefCell, rc::Rc};
use winarchy_browser::library::{Library, Suggestion};
use winarchy_browser::tabs::{self, Tab, TabId, Tabs};
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
    // Empty tab badges are normal. Do not pass an empty Vec's dangling UTF-16
    // pointer to User32: DrawTextW can dereference it even with a zero count.
    if value.is_empty() {
        return;
    }
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
    pub tabs_mode: bool,
    tabs: Rc<RefCell<Tabs>>,
    tab_rows: Vec<Tab>,
    theme: winarchy_theme::Theme,
    status: RefCell<String>,
}
impl Picker {
    pub unsafe fn new(
        parent: HWND,
        instance: HINSTANCE,
        library: Rc<RefCell<Library>>,
        tabs: Rc<RefCell<Tabs>>,
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
            tabs_mode: false,
            tabs,
            tab_rows: Vec::new(),
            theme: winarchy_theme::Theme::current(&winarchy_theme::config_home()),
            status: RefCell::new(String::new()),
        };
        picker.set_font(parent);
        SendMessageW(
            edit,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(w!("Search or enter an address…").0 as isize)),
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
        // Compact the original palette geometry by 20%, retaining DPI scaling.
        unsafe { (n * GetDpiForWindow(self.panel) as i32 * 4 / (96 * 5)).max(1) }
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
        self.tabs_mode = false;
        SendMessageW(
            self.edit,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(w!("Search or enter an address…").0 as isize)),
        );
        let _ = SetWindowTextW(self.panel, w!("Navigation"));
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
    pub unsafe fn show_tabs(&mut self, parent: HWND) {
        self.tabs_mode = true;
        self.visible = true;
        self.status.borrow_mut().clear();
        let _ = SetWindowTextW(self.panel, w!("Open tabs"));
        SendMessageW(
            self.edit,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(w!("Fuzzy-find an open tab…").0 as isize)),
        );
        let _ = SetWindowTextW(self.edit, w!(""));
        self.refresh_tabs(parent, false);
        let _ = ShowWindow(self.panel, SW_SHOWNA);
        let _ = SetFocus(Some(self.edit));
    }
    pub unsafe fn selected_tab(&self) -> Option<TabId> {
        if !self.tabs_mode {
            return None;
        }
        let index = SendMessageW(self.list, LB_GETCURSEL, None, None).0;
        self.tab_rows
            .get(usize::try_from(index).ok()?)
            .map(|tab| tab.id)
    }
    pub unsafe fn refresh_tabs(&mut self, parent: HWND, preserve_selection: bool) {
        if !self.tabs_mode {
            return;
        }
        let previous = self.selected_tab();
        let index = SendMessageW(self.list, LB_GETCURSEL, None, None).0.max(0) as usize;
        let query = self.text();
        self.tab_rows = self.tabs.borrow().search(&query);
        let preferred = if preserve_selection {
            previous
        } else if query.is_empty() {
            self.tabs.borrow().active()
        } else {
            None
        };
        let selection = preferred
            .and_then(|id| self.tab_rows.iter().position(|tab| tab.id == id))
            .unwrap_or(if preserve_selection {
                index.min(self.tab_rows.len().saturating_sub(1))
            } else {
                0
            });
        SendMessageW(self.list, WM_SETREDRAW, Some(WPARAM(0)), None);
        SendMessageW(self.list, LB_RESETCONTENT, None, None);
        for tab in &self.tab_rows {
            let value = wide(&format!(
                "{}{}{}{} — {}",
                if self.tabs.borrow().active() == Some(tab.id) {
                    "[active] "
                } else {
                    ""
                },
                if tab.pinned { "[pinned] " } else { "" },
                if tab.muted {
                    "[muted] "
                } else if tab.audible {
                    "[audio] "
                } else {
                    ""
                },
                tab.label(),
                tab.url
            ));
            SendMessageW(
                self.list,
                LB_ADDSTRING,
                None,
                Some(LPARAM(value.as_ptr() as isize)),
            );
        }
        if !self.tab_rows.is_empty() {
            SendMessageW(self.list, LB_SETCURSEL, Some(WPARAM(selection)), None);
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
    fn row_count(&self) -> usize {
        if self.tabs_mode {
            self.tab_rows.len()
        } else {
            self.suggestions.len()
        }
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
        if self.tabs_mode {
            self.refresh_tabs(parent, false);
            return;
        }
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
        let row_height = if self.tabs_mode { 64 } else { 88 };
        let height = p(172 + self.row_count().clamp(1, 8) as i32 * row_height)
            .min((r.bottom - p(32)).max(1));
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
            (width - p(if self.tabs_mode { 210 } else { 130 })).max(1),
            p(28),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(
            self.list,
            LB_SETITEMHEIGHT,
            Some(WPARAM(0)),
            Some(LPARAM(p(row_height) as isize)),
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
            if self.row_count() == 0 {
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
            if self.tabs_mode {
                "WINARCHY BROWSER › Open tabs"
            } else {
                "WINARCHY BROWSER › Navigation"
            },
            &self.theme.accent,
        );
        if self.tabs_mode {
            label(
                r.right - p(155),
                p(24),
                r.right - p(80),
                p(28),
                &format!(
                    "{}/{}",
                    self.tab_rows.len(),
                    self.tabs.borrow().entries().len()
                ),
                &self.theme.subtext,
            );
        }
        if self.row_count() == 0 {
            label(
                p(24),
                p(120),
                r.right - p(24),
                p(40),
                if self.tabs_mode {
                    "No matching tabs"
                } else {
                    "No local results · Enter to search"
                },
                &self.theme.subtext,
            );
        }
        let status = self.status.borrow();
        let footer = if self.tabs_mode {
            "Open tabs · ↑↓ select · Enter switch · Ctrl+W close".into()
        } else if status.is_empty() {
            format!(
                "{} results · ↑↓ select · Enter open",
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
        if self.tabs_mode {
            self.draw_tab(item);
            return;
        }
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
        let category = if s.bookmarked { "BOOKMARK" } else { "HISTORY" };
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
                if s.bookmarked { "Bookmarks" } else { "History" },
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
    unsafe fn draw_tab(&self, item: &DRAWITEMSTRUCT) {
        let Some(tab) = self.tab_rows.get(item.itemID as usize) else {
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
        let label = |x, y, right, height, value: &str, tint: &str| {
            text(
                item.hDC,
                RECT {
                    left: x,
                    top: y,
                    right,
                    bottom: y + height,
                },
                value,
                color(tint),
            )
        };
        let active = self.tabs.borrow().active() == Some(tab.id);
        let state = format!(
            "{}{}{}",
            if tab.pinned { "◆ " } else { "" },
            if tab.muted {
                "muted "
            } else if tab.audible {
                "♫ "
            } else {
                ""
            },
            if active { "↵" } else { "" }
        );
        label(
            r.left + p(12),
            r.top + p(6),
            r.right - p(150),
            p(28),
            tab.label(),
            if selected {
                &self.theme.accent
            } else {
                &self.theme.text
            },
        );
        label(
            r.right - p(140),
            r.top + p(6),
            r.right - p(12),
            p(28),
            &state,
            &self.theme.accent,
        );
        let url = if tab.home {
            "Home"
        } else {
            tab.url
                .strip_prefix("https://")
                .or_else(|| tab.url.strip_prefix("http://"))
                .unwrap_or(&tab.url)
        };
        label(
            r.left + p(12),
            r.top + p(34),
            r.right - p(150),
            p(24),
            url,
            &self.theme.subtext,
        );
        label(
            r.right - p(140),
            r.top + p(34),
            r.right - p(12),
            p(24),
            &tab.age(tabs::now()),
            &self.theme.subtext,
        );
        SelectObject(item.hDC, old);
    }
    pub unsafe fn choose(&self, direction: i32) {
        let current = SendMessageW(self.list, LB_GETCURSEL, None, None).0 as i32;
        let last = self.row_count() as i32 - 1;
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
