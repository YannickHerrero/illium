//! Native edit + suggestion list, reused on the home surface and by Ctrl+L.
#![allow(unsafe_op_in_unsafe_fn)]
use super::native::wide;
use std::{cell::RefCell, rc::Rc};
use winarchy_browser::library::{Library, Suggestion};
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
const LABEL_ID: usize = 103;
pub struct Picker {
    pub edit: HWND,
    pub list: HWND,
    label: HWND,
    font: HFONT,
    pub visible: bool,
    pub home: bool,
    pub library: Rc<RefCell<Library>>,
    suggestions: Vec<Suggestion>,
}
impl Picker {
    pub unsafe fn new(
        parent: HWND,
        instance: HINSTANCE,
        library: Rc<RefCell<Library>>,
    ) -> Result<Self> {
        let child = |class: PCWSTR, text: PCWSTR, style: WINDOW_STYLE, id: usize| {
            CreateWindowExW(
                Default::default(),
                class,
                text,
                WS_CHILD | style,
                0,
                0,
                1,
                1,
                Some(parent),
                Some(HMENU(id as *mut _)),
                Some(instance),
                None,
            )
        };
        let edit = child(
            w!("EDIT"),
            w!(""),
            WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            EDIT_ID,
        )?;
        // No sorted style: ordering comes from the fuzzy matcher, not Windows.
        let list = child(
            w!("LISTBOX"),
            w!(""),
            WINDOW_STYLE((LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
            LIST_ID,
        )?;
        let label = child(
            w!("STATIC"),
            w!("URL ou recherche DuckDuckGo"),
            WINDOW_STYLE(0),
            LABEL_ID,
        )?;
        let mut picker = Self {
            edit,
            list,
            label,
            font: HFONT::default(),
            visible: false,
            home: false,
            library,
            suggestions: Vec::new(),
        };
        picker.set_font(parent);
        SendMessageW(
            edit,
            0x1501, /* EM_SETCUEBANNER */
            Some(WPARAM(1)),
            Some(LPARAM(
                w!("Saisissez une URL ou une recherche DuckDuckGo…").0 as isize,
            )),
        );
        SendMessageW(
            edit,
            0x00C5, /* EM_LIMITTEXT */
            Some(WPARAM(4096)),
            None,
        );
        Ok(picker)
    }
    pub unsafe fn set_font(&mut self, parent: HWND) {
        let height = -(18 * GetDpiForWindow(parent) as i32 / 96);
        let font = CreateFontW(
            height,
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
            w!("Segoe UI"),
        );
        if font.is_invalid() {
            return;
        }
        for hwnd in [self.edit, self.list, self.label] {
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
        self.status("URL ou recherche DuckDuckGo · ↓ suggestions · Ctrl+D favori");
        self.refresh(parent);
        let _ = ShowWindow(self.edit, SW_SHOW);
        let _ = ShowWindow(self.label, SW_SHOW);
        let _ = SetFocus(Some(self.edit));
        SendMessageW(
            self.edit,
            0x00B1, /* EM_SETSEL */
            Some(WPARAM(0)),
            Some(LPARAM(-1)),
        );
    }
    pub unsafe fn hide(&mut self) {
        self.visible = false;
        for hwnd in [self.edit, self.list, self.label] {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    pub unsafe fn status(&self, text: &str) {
        let _ = SetWindowTextW(self.label, PCWSTR(wide(text).as_ptr()));
    }
    pub unsafe fn refresh(&mut self, parent: HWND) {
        self.suggestions = self.library.borrow().suggestions(&self.text());
        SendMessageW(self.list, WM_SETREDRAW, Some(WPARAM(0)), None);
        SendMessageW(self.list, LB_RESETCONTENT, None, None);
        for suggestion in &self.suggestions {
            let marker = if suggestion.bookmarked { "★" } else { "↶" };
            let title = if suggestion.site.title.is_empty() {
                &suggestion.site.url
            } else {
                &suggestion.site.title
            };
            let display = wide(&format!("{marker} {title} — {}", suggestion.site.url));
            SendMessageW(
                self.list,
                LB_ADDSTRING,
                None,
                Some(LPARAM(display.as_ptr() as isize)),
            );
        }
        SendMessageW(self.list, WM_SETREDRAW, Some(WPARAM(1)), None);
        // Enter submits typed input unless a suggestion was explicitly selected.
        self.layout(parent);
        let _ = InvalidateRect(Some(self.list), None, true);
        // Erase the area vacated when results shrink or the picker is resized.
        // The parent clips child controls, so this does not paint over the WebView.
        let _ = InvalidateRect(Some(parent), None, true);
    }
    pub unsafe fn layout(&self, parent: HWND) -> i32 {
        let mut rect = RECT::default();
        let _ = GetClientRect(parent, &mut rect);
        let scale = GetDpiForWindow(parent) as i32;
        let px = |n: i32| (n * scale / 96).max(1);
        let margin = px(16);
        let width = px(700).min((rect.right - margin * 2).max(1));
        let x = (rect.right - width) / 2;
        let row = px(30);
        // Keep the input centered regardless of the number of matches.
        let y = if self.home {
            ((rect.bottom - row) / 2).max(margin + row)
        } else {
            margin + row
        };
        let _ = SetWindowPos(
            self.label,
            Some(HWND_TOP),
            x,
            y - row,
            width,
            row,
            SWP_NOACTIVATE,
        );
        let _ = SetWindowPos(self.edit, Some(HWND_TOP), x, y, width, row, SWP_NOACTIVATE);
        let height =
            (self.suggestions.len() as i32 * row).min((rect.bottom - y - row - margin).max(0));
        SendMessageW(
            self.list,
            LB_SETITEMHEIGHT,
            Some(WPARAM(0)),
            Some(LPARAM(row as isize)),
        );
        let _ = SetWindowPos(
            self.list,
            Some(HWND_TOP),
            x,
            y + row + px(4),
            width,
            height,
            SWP_NOACTIVATE,
        );
        let _ = ShowWindow(
            self.list,
            if self.visible && height > 0 {
                SW_SHOWNA
            } else {
                SW_HIDE
            },
        );
        if self.visible {
            y + row + height + margin
        } else {
            0
        }
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
