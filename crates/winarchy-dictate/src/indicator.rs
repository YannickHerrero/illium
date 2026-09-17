//! A small themed pill under the bar, centered on the monitor holding the
//! cursor: it shows that the microphone is open, then that the model is
//! working, and disappears once the text has been pasted. A tool window that
//! never takes focus, so Winarchy neither tiles nor activates it.
use std::sync::Mutex;
use winarchy_theme::Theme;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Hidden,
    Downloading,
    Loading,
    Listening {
        level: f32,
    },
    Transcribing,
    /// Shown for a moment, then hidden.
    Notice(String),
}
static STATE: Mutex<State> = Mutex::new(State::Hidden);
const WM_STATE: u32 = WM_APP + 1;
const NOTICE_TIMER: usize = 1;
const WIDTH: i32 = 260;
const HEIGHT: i32 = 44;
/// Thread-safe way to drive the window from the controller.
#[derive(Clone, Copy)]
pub struct Handle(isize);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Handle {
    pub fn set(&self, state: State) {
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = state;
        unsafe {
            let _ = PostMessageW(Some(HWND(self.0 as *mut _)), WM_STATE, WPARAM(0), LPARAM(0));
        }
    }
    pub fn quit(&self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.0 as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn color(hex: &str) -> COLORREF {
    let (r, g, b) = winarchy_theme::rgb(hex).unwrap_or((255, 0, 255));
    COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16)
}
/// Creates the hidden window on the calling thread, which must then run
/// [`run_loop`].
pub fn create() -> Result<Handle, String> {
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let class = wide("WinarchyDictate");
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        }) == 0
        {
            return Err("indicator window class registration failed".into());
        }
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            w!("Winarchy dictation"),
            WS_POPUP,
            0,
            0,
            WIDTH,
            HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("indicator window failed: {e}"))?;
        Ok(Handle(window.0 as isize))
    }
}
pub fn run_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
/// Places the pill under the top edge of the work area of the cursor's monitor.
unsafe fn place(window: HWND) {
    unsafe {
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let dpi = GetDpiForWindow(window).max(96) as i32;
        let scale = |v: i32| v * dpi / 96;
        let (width, height) = (scale(WIDTH), scale(HEIGHT));
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - width) / 2;
        let y = work.top + scale(12);
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let radius = scale(10);
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
        let _ = SetWindowRgn(window, Some(region), true);
    }
}
unsafe fn paint(window: HWND) {
    unsafe {
        let state = STATE.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let theme = Theme::current(&winarchy_theme::config_home());
        let dpi = GetDpiForWindow(window).max(96) as i32;
        let scale = |v: i32| v * dpi / 96;
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut ps);
        let mut rect = RECT::default();
        let _ = GetClientRect(window, &mut rect);
        let background = CreateSolidBrush(color(&theme.surface));
        FillRect(dc, &rect, background);
        let _ = DeleteObject(background.into());
        let border = CreateSolidBrush(color(&theme.overlay));
        FrameRect(dc, &rect, border);
        let _ = DeleteObject(border.into());
        let (dot, text, level) = match &state {
            State::Hidden => (theme.subtext.clone(), String::new(), None),
            State::Downloading => (
                theme.yellow.clone(),
                format!("Downloading the model ({} MB)…", crate::model::SIZE_MB),
                None,
            ),
            State::Loading => (theme.yellow.clone(), "Loading the model…".into(), None),
            State::Listening { level } => (theme.red.clone(), "Listening…".into(), Some(*level)),
            State::Transcribing => (theme.accent.clone(), "Transcribing…".into(), None),
            State::Notice(message) => (theme.subtext.clone(), message.clone(), None),
        };
        // Dot on the left, text beside it, level bar on the right while listening.
        let dot_brush = CreateSolidBrush(color(&dot));
        let old_brush = SelectObject(dc, dot_brush.into());
        let pen = CreatePen(PS_NULL, 0, COLORREF(0));
        let old_pen = SelectObject(dc, pen.into());
        let (dot_size, pad) = (scale(10), scale(16));
        let top = (rect.bottom - dot_size) / 2;
        let _ = Ellipse(dc, pad, top, pad + dot_size, top + dot_size);
        let font = CreateFontW(
            -scale(13),
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color(&theme.text));
        let bar_width = if level.is_some() { scale(70) } else { 0 };
        let mut text_rect = RECT {
            left: pad + dot_size + scale(10),
            top: rect.top,
            right: rect.right - pad - bar_width - if bar_width > 0 { scale(10) } else { 0 },
            bottom: rect.bottom,
        };
        let mut wide_text = wide(&text);
        wide_text.pop();
        DrawTextW(
            dc,
            &mut wide_text,
            &mut text_rect,
            DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        if let Some(level) = level {
            let track = RECT {
                left: rect.right - pad - bar_width,
                top: (rect.bottom - scale(6)) / 2,
                right: rect.right - pad,
                bottom: (rect.bottom + scale(6)) / 2,
            };
            let track_brush = CreateSolidBrush(color(&theme.overlay));
            FillRect(dc, &track, track_brush);
            let _ = DeleteObject(track_brush.into());
            // RMS of speech sits far below full scale; stretch it so the bar moves.
            let filled = ((level * 4.0).min(1.0) * bar_width as f32) as i32;
            let fill = RECT {
                right: track.left + filled,
                ..track
            };
            let fill_brush = CreateSolidBrush(color(&theme.accent));
            FillRect(dc, &fill, fill_brush);
            let _ = DeleteObject(fill_brush.into());
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(dot_brush.into());
        let _ = EndPaint(window, &ps);
    }
}
unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_STATE => {
                let state = STATE.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let _ = KillTimer(Some(window), NOTICE_TIMER);
                if state == State::Hidden {
                    let _ = ShowWindow(window, SW_HIDE);
                } else {
                    if !IsWindowVisible(window).as_bool() {
                        place(window);
                    }
                    if matches!(state, State::Notice(_)) {
                        SetTimer(Some(window), NOTICE_TIMER, 2200, None);
                    }
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            WM_TIMER if w.0 == NOTICE_TIMER => {
                let _ = KillTimer(Some(window), NOTICE_TIMER);
                *STATE.lock().unwrap_or_else(|e| e.into_inner()) = State::Hidden;
                let _ = ShowWindow(window, SW_HIDE);
                LRESULT(0)
            }
            WM_PAINT => {
                paint(window);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_CLOSE => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, w, l),
        }
    }
}
