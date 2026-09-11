//! Region screenshot to the clipboard: freeze the virtual screen, let the user
//! drag a rectangle over a dimmed copy, put the selection on the clipboard as a
//! bitmap. Kept out of the daemon so the process holding the keyboard hook
//! contains no screen-capture code.
#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    if let Err(e) = capture::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchy-shot requires Windows");
        std::process::exit(1);
    }
}
#[cfg(windows)]
mod capture {
    use std::cell::RefCell;
    use windows::{
        Win32::{
            Foundation::*,
            Graphics::Gdi::*,
            System::{DataExchange::*, LibraryLoader::GetModuleHandleW},
            UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        },
        core::PCWSTR,
    };
    const CF_BITMAP: u32 = 2;
    const VK_ESCAPE: usize = 0x1b;
    struct State {
        size: (i32, i32),
        screen: HDC,
        frozen: HDC,
        dimmed: HDC,
        start: Option<POINT>,
        current: POINT,
        cancelled: bool,
    }
    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }
    fn err(context: &str) -> String {
        format!("{context}: {}", windows::core::Error::from_win32())
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn point(l: LPARAM) -> POINT {
        POINT {
            x: (l.0 & 0xffff) as i16 as i32,
            y: ((l.0 >> 16) & 0xffff) as i16 as i32,
        }
    }
    fn selection(a: POINT, b: POINT) -> RECT {
        RECT {
            left: a.x.min(b.x),
            top: a.y.min(b.y),
            right: a.x.max(b.x) + 1,
            bottom: a.y.max(b.y) + 1,
        }
    }
    /// Memory DC holding a copy of the screen area, optionally darkened.
    unsafe fn snapshot(
        screen: HDC,
        origin: POINT,
        (w, h): (i32, i32),
        dim: bool,
    ) -> Result<HDC, String> {
        unsafe {
            let dc = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            SelectObject(dc, bitmap.into());
            BitBlt(
                dc,
                0,
                0,
                w,
                h,
                Some(screen),
                origin.x,
                origin.y,
                ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
            )
            .map_err(|e| format!("screen copy failed: {e}"))?;
            if dim {
                let black = CreateCompatibleDC(Some(screen));
                let pixel = CreateCompatibleBitmap(screen, 1, 1);
                SelectObject(black, pixel.into());
                let _ = PatBlt(black, 0, 0, 1, 1, BLACKNESS);
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    SourceConstantAlpha: 110,
                    ..Default::default()
                };
                let _ = AlphaBlend(dc, 0, 0, w, h, black, 0, 0, 1, 1, blend);
                let _ = DeleteDC(black);
                let _ = DeleteObject(pixel.into());
            }
            Ok(dc)
        }
    }
    unsafe fn copy_to_clipboard(h: HWND, s: &State, r: RECT) -> Result<(), String> {
        unsafe {
            let (w, hgt) = (r.right - r.left, r.bottom - r.top);
            let dc = CreateCompatibleDC(Some(s.screen));
            let bitmap = CreateCompatibleBitmap(s.screen, w, hgt);
            let previous = SelectObject(dc, bitmap.into());
            let copied = BitBlt(dc, 0, 0, w, hgt, Some(s.frozen), r.left, r.top, SRCCOPY);
            SelectObject(dc, previous);
            let _ = DeleteDC(dc);
            copied.map_err(|e| format!("selection copy failed: {e}"))?;
            OpenClipboard(Some(h)).map_err(|e| format!("clipboard busy: {e}"))?;
            let result = EmptyClipboard().map_err(|e| e.to_string()).and_then(|()| {
                SetClipboardData(CF_BITMAP, Some(HANDLE(bitmap.0)))
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
            let _ = CloseClipboard();
            if result.is_err() {
                let _ = DeleteObject(bitmap.into());
            }
            result.map_err(|e| format!("clipboard write failed: {e}"))
        }
    }
    unsafe fn paint(h: HWND, s: &State) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let target = BeginPaint(h, &mut ps);
            let (w, hgt) = s.size;
            let back = CreateCompatibleDC(Some(target));
            let buffer = CreateCompatibleBitmap(target, w, hgt);
            SelectObject(back, buffer.into());
            let _ = BitBlt(back, 0, 0, w, hgt, Some(s.dimmed), 0, 0, SRCCOPY);
            if let Some(start) = s.start {
                let r = selection(start, s.current);
                let _ = BitBlt(
                    back,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    Some(s.frozen),
                    r.left,
                    r.top,
                    SRCCOPY,
                );
                let brush = CreateSolidBrush(COLORREF(0x00fa_b489));
                let border = RECT {
                    left: r.left - 1,
                    top: r.top - 1,
                    right: r.right + 1,
                    bottom: r.bottom + 1,
                };
                FrameRect(back, &border, brush);
                let _ = DeleteObject(brush.into());
            }
            let _ = BitBlt(target, 0, 0, w, hgt, Some(back), 0, 0, SRCCOPY);
            let _ = DeleteDC(back);
            let _ = DeleteObject(buffer.into());
            let _ = EndPaint(h, &ps);
        }
    }
    unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let handled = STATE.with_borrow_mut(|state| {
            let Some(s) = state.as_mut() else {
                return false;
            };
            match m {
                WM_PAINT => unsafe { paint(h, s) },
                WM_SETCURSOR => unsafe {
                    SetCursor(LoadCursorW(None, IDC_CROSS).ok());
                },
                WM_LBUTTONDOWN => unsafe {
                    s.start = Some(point(l));
                    s.current = point(l);
                    SetCapture(h);
                    let _ = InvalidateRect(Some(h), None, false);
                },
                WM_MOUSEMOVE if s.start.is_some() => unsafe {
                    s.current = point(l);
                    let _ = InvalidateRect(Some(h), None, false);
                },
                WM_LBUTTONUP if s.start.is_some() => unsafe {
                    let _ = ReleaseCapture();
                    let r = selection(s.start.take().unwrap_or(s.current), point(l));
                    if let Err(e) = copy_to_clipboard(h, s, r) {
                        eprintln!("{e}");
                        s.cancelled = true;
                    }
                    PostQuitMessage(0);
                },
                WM_RBUTTONDOWN => unsafe {
                    s.cancelled = true;
                    PostQuitMessage(0);
                },
                WM_KEYDOWN if w.0 == VK_ESCAPE => unsafe {
                    s.cancelled = true;
                    PostQuitMessage(0);
                },
                // Losing activation mid-capture would leave a stale overlay.
                WM_KILLFOCUS => unsafe {
                    s.cancelled = true;
                    PostQuitMessage(0);
                },
                _ => return false,
            }
            true
        });
        if handled {
            LRESULT(0)
        } else {
            unsafe { DefWindowProcW(h, m, w, l) }
        }
    }
    pub fn run() -> Result<(), String> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let origin = POINT {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            };
            let size = (
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            );
            if size.0 <= 0 || size.1 <= 0 {
                return Err("no display".into());
            }
            let screen = GetDC(None);
            let frozen = snapshot(screen, origin, size, false)?;
            let dimmed = snapshot(screen, origin, size, true)?;
            STATE.set(Some(State {
                size,
                screen,
                frozen,
                dimmed,
                start: None,
                current: POINT::default(),
                cancelled: false,
            }));
            let class = wide("WinarchyShot");
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
                ..Default::default()
            });
            let window = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WS_POPUP | WS_VISIBLE,
                origin.x,
                origin.y,
                size.0,
                size.1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|e| err(&format!("overlay window failed: {e}")))?;
            let _ = SetForegroundWindow(window);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = DestroyWindow(window);
            let cancelled = STATE.with_borrow_mut(|state| {
                let s = state.take();
                if let Some(s) = &s {
                    let _ = DeleteDC(s.frozen);
                    let _ = DeleteDC(s.dimmed);
                    ReleaseDC(None, s.screen);
                }
                s.is_none_or(|s| s.cancelled)
            });
            if cancelled {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}
