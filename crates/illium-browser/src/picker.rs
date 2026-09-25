//! Slint navigation/tab palette with an opaque native EDIT for Windows text input.
//! The hidden LISTBOX retains native selection and automation data;
//! Slint draws the rows with per-pixel alpha, never a global window opacity.
#![allow(unsafe_op_in_unsafe_fn)]
use super::native::wide;
use crate::surface::{PaletteRow, Surface, place_popup};
use illium_browser::library::{Library, Suggestion};
use illium_browser::tabs::{self, Tab, TabId, Tabs};
use slint::{ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};
pub const EDIT_ID: usize = 101;
pub const LIST_ID: usize = 102;
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND | WM_CTLCOLOREDIT => SendMessageW(
            GetWindow(hwnd, GW_OWNER).unwrap_or_default(),
            msg,
            Some(wp),
            Some(lp),
        ),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            super::native::paint_picker();
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEWHEEL => {
            super::native::picker_pointer(msg, wp, lp);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub struct Picker {
    pub panel: HWND,
    pub edit: HWND,
    pub list: HWND,
    font: HFONT,
    line_height: i32,
    pub visible: bool,
    pub home: bool,
    pub library: Rc<RefCell<Library>>,
    suggestions: Vec<Suggestion>,
    pub tabs_mode: bool,
    tabs: Rc<RefCell<Tabs>>,
    tab_rows: Vec<Tab>,
    theme: illium_theme::Theme,
    surface: RefCell<Surface>,
    status: RefCell<String>,
}
impl Picker {
    pub unsafe fn new(
        parent: HWND,
        instance: HINSTANCE,
        library: Rc<RefCell<Library>>,
        tabs: Rc<RefCell<Tabs>>,
    ) -> Result<Self> {
        let class = w!("IlliumNavigationPalette");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(panel_proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let panel = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("Navigation"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            None,
        )?;
        let edit = CreateWindowExW(
            WS_EX_LAYERED,
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            0,
            0,
            1,
            1,
            Some(panel),
            Some(HMENU(EDIT_ID as *mut _)),
            Some(instance),
            None,
        )?;
        // GDI input has its own opaque child surface; it must not draw into
        // the palette's absent redirection bitmap or inherit theme opacity.
        SetLayeredWindowAttributes(edit, COLORREF(0), 255, LWA_ALPHA)?;
        let list = CreateWindowExW(
            Default::default(),
            w!("LISTBOX"),
            w!(""),
            WS_CHILD | WINDOW_STYLE(LBS_HASSTRINGS as u32),
            0,
            0,
            1,
            1,
            Some(panel),
            Some(HMENU(LIST_ID as *mut _)),
            Some(instance),
            None,
        )?;
        let surface = Surface::new(panel, 1)?;
        surface.ui.on_activate(move |index| {
            SendMessageW(list, LB_SETCURSEL, Some(WPARAM(index as usize)), None);
            let _ = PostMessageW(Some(parent), super::native::SUBMIT, WPARAM(0), LPARAM(0));
        });
        let theme = illium_theme::Theme::current(&illium_theme::config_home());
        let mut picker = Self {
            panel,
            edit,
            list,
            font: HFONT::default(),
            line_height: 13,
            visible: false,
            home: false,
            library,
            suggestions: vec![],
            tabs_mode: false,
            tabs,
            tab_rows: vec![],
            theme: theme.clone(),
            surface: RefCell::new(surface),
            status: RefCell::new(String::new()),
        };
        picker.set_font(parent);
        picker.set_theme(&theme);
        SendMessageW(edit, 0x00C5, Some(WPARAM(4096)), None);
        Ok(picker)
    }
    pub unsafe fn set_theme(&mut self, theme: &illium_theme::Theme) {
        self.theme = theme.clone();
        self.surface.borrow().theme(theme);
        illium_theme::blur::set(self.panel.0 as isize, theme.background_blur);
        let _ = RedrawWindow(
            Some(self.panel),
            None,
            None,
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
    fn px(&self, n: i32) -> i32 {
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
        SendMessageW(
            self.edit,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        if !self.font.is_invalid() {
            let _ = DeleteObject(self.font.into());
        }
        self.font = font;
        // A single-line EDIT only paints its font line reliably on its layered
        // child surface. Give it exactly that height; Slint supplies the padding
        // and rounded field background instead of exposing an unpainted strip.
        self.line_height = (13 * GetDpiForWindow(parent) as i32 / 96).max(1);
        let dc = GetDC(Some(self.edit));
        if !dc.is_invalid() {
            let previous = SelectObject(dc, font.into());
            let mut metrics = TEXTMETRICW::default();
            if GetTextMetricsW(dc, &mut metrics).as_bool() {
                self.line_height = metrics.tmHeight.max(1);
            }
            SelectObject(dc, previous);
            ReleaseDC(Some(self.edit), dc);
        }
    }
    pub unsafe fn text(&self) -> String {
        let mut text = vec![0; GetWindowTextLengthW(self.edit) as usize + 1];
        let len = GetWindowTextW(self.edit, &mut text);
        String::from_utf16_lossy(&text[..len as usize])
    }
    pub unsafe fn show(&mut self, parent: HWND, home: bool, current: &str) {
        self.tabs_mode = false;
        self.visible = true;
        self.home = home;
        let _ = SetWindowTextW(self.panel, w!("Navigation"));
        SendMessageW(
            self.edit,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(w!("Search or enter an address…").0 as isize)),
        );
        if let Err(e) = self.library.borrow_mut().reload() {
            eprintln!("Cannot reload history: {e}");
        }
        let _ = SetWindowTextW(self.edit, PCWSTR(wide(current).as_ptr()));
        self.status.borrow_mut().clear();
        self.refresh(parent);
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
        self.tab_rows = self.tabs.borrow().search(&self.text());
        let preferred = if preserve_selection {
            previous
        } else if self.text().is_empty() {
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
        self.update_scene();
        self.layout(parent);
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
        self.update_scene();
    }
    pub unsafe fn refresh(&mut self, parent: HWND) {
        if self.tabs_mode {
            self.refresh_tabs(parent, false);
            return;
        }
        self.suggestions = self.library.borrow().suggestions(&self.text());
        self.suggestions.sort_by_key(|s| !s.bookmarked);
        self.status.borrow_mut().clear();
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
        self.update_scene();
        self.layout(parent);
    }
    unsafe fn update_scene(&self) {
        let surface = self.surface.borrow();
        let ui = &surface.ui;
        let rows: Vec<PaletteRow> = if self.tabs_mode {
            self.tab_rows
                .iter()
                .map(|tab| PaletteRow {
                    title: tab.label().into(),
                    detail: if tab.home {
                        "Home".into()
                    } else {
                        tab.url.as_str().into()
                    },
                    badge: format!(
                        "{}{}{}",
                        if tab.pinned { "◆ " } else { "" },
                        if tab.muted {
                            "muted "
                        } else if tab.audible {
                            "♫ "
                        } else {
                            ""
                        },
                        if self.tabs.borrow().active() == Some(tab.id) {
                            "active"
                        } else {
                            ""
                        }
                    )
                    .into(),
                    group: tab.age(tabs::now()).into(),
                })
                .collect()
        } else {
            self.suggestions
                .iter()
                .enumerate()
                .map(|(i, s)| PaletteRow {
                    title: format!("{} {}", if s.bookmarked { "★" } else { "◷" }, s.site.url)
                        .into(),
                    detail: s.description(tabs::now()).into(),
                    badge: if s.bookmarked {
                        "BOOKMARK".into()
                    } else {
                        "HISTORY".into()
                    },
                    group: if i == 0 || self.suggestions[i - 1].bookmarked != s.bookmarked {
                        if s.bookmarked {
                            "Bookmarks".into()
                        } else {
                            "History".into()
                        }
                    } else {
                        "".into()
                    },
                })
                .collect()
        };
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_heading(if self.tabs_mode {
            "ILLIUM BROWSER › Open tabs".into()
        } else {
            "ILLIUM BROWSER › Navigation".into()
        });
        ui.set_row_height(70.4);
        ui.set_scroll_offset(0.);
        ui.set_selected(SendMessageW(self.list, LB_GETCURSEL, None, None).0 as i32);
        ui.set_footer(if !self.status.borrow().is_empty() {
            self.status.borrow().as_str().into()
        } else if self.tabs_mode {
            "↑↓ select · Enter switch · Ctrl+W close".into()
        } else {
            format!("{} results · ↑↓ select · Enter open", self.row_count()).into()
        });
        let _ = InvalidateRect(Some(self.panel), None, false);
    }
    pub unsafe fn layout(&self, parent: HWND) -> i32 {
        let mut r = RECT::default();
        let _ = GetClientRect(parent, &mut r);
        let p = |n| self.px(n);
        let width = p(850).min((r.right - p(32)).max(1));
        let height =
            p(172 + self.row_count().clamp(1, 8) as i32 * 88).min((r.bottom - p(32)).max(1));
        place_popup(
            self.panel,
            parent,
            (r.right - width) / 2,
            (r.bottom - height) / 2,
            width,
            height,
            self.visible,
        );
        let _ = SetWindowPos(
            self.edit,
            None,
            p(60),
            p(15) + (p(45) - self.line_height) / 2,
            (width - p(155)).max(1),
            self.line_height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = InvalidateRect(Some(self.panel), None, false);
        0
    }
    #[cfg(test)]
    pub unsafe fn assert_alpha(&self, background: u8) {
        self.surface.borrow_mut().assert_alpha(background, true);
    }
    pub unsafe fn paint(&self) {
        if let Err(e) = self.surface.borrow_mut().paint() {
            eprintln!("Palette composition failed: {e}");
        }
    }
    pub unsafe fn pointer(&self, msg: u32, wp: WPARAM, lp: LPARAM) {
        let surface = self.surface.borrow();
        if msg == WM_MOUSEWHEEL {
            let delta = ((wp.0 >> 16) as i16) as f32 / 120. * 70.4;
            let mut r = RECT::default();
            let _ = GetClientRect(self.panel, &mut r);
            let height = r.bottom as f32 * 96. / GetDpiForWindow(self.panel) as f32 - 137.;
            let max = (self.row_count() as f32 * 70.4 - height).max(0.);
            surface
                .ui
                .set_scroll_offset((surface.ui.get_scroll_offset() - delta).clamp(0., max));
        } else {
            surface.pointer(msg, lp);
        }
        let _ = InvalidateRect(Some(self.panel), None, false);
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
        let surface = self.surface.borrow();
        surface.ui.set_selected(next);
        let mut r = RECT::default();
        let _ = GetClientRect(self.panel, &mut r);
        let height = (r.bottom as f32 * 96. / GetDpiForWindow(self.panel) as f32 - 137.).max(1.);
        let offset = surface.ui.get_scroll_offset();
        if next as f32 * 70.4 < offset {
            surface.ui.set_scroll_offset(next as f32 * 70.4);
        } else if (next + 1) as f32 * 70.4 > offset + height {
            surface
                .ui
                .set_scroll_offset(((next + 1) as f32 * 70.4 - height).max(0.));
        }
        let _ = InvalidateRect(Some(self.panel), None, false);
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
