//! Region screenshot to the clipboard: freeze the virtual screen, let the user
//! drag a rectangle over a dimmed copy, put the selection on the clipboard as a
//! bitmap.
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};
const CF_DIB: u32 = 8;
const VK_ESCAPE: usize = 0x1b;
/// A warmed capture thread in the existing resident process.
pub struct Resident {
    sender: std::sync::mpsc::SyncSender<()>,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Resident {
    pub fn new() -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_busy = busy.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("screenshot".into())
            .spawn(move || {
                let ready = prepare();
                let failed = ready.is_err();
                let _ = ready_tx.send(ready);
                if failed {
                    return;
                }
                while rx.recv().is_ok() {
                    if let Err(error) = capture() {
                        crate::log::write(&format!("shot: {error}"));
                    }
                    worker_busy.store(false, std::sync::atomic::Ordering::Release);
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { sender: tx, busy })
    }
    pub fn show(&self) -> Result<(), String> {
        if self.busy.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Ok(());
        }
        match self.sender.try_send(()) {
            Ok(()) | Err(std::sync::mpsc::TrySendError::Full(())) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Disconnected(())) => {
                self.busy.store(false, std::sync::atomic::Ordering::Release);
                Err("screenshot worker stopped".into())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::Resident;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    #[test]
    fn requests_coalesce_until_capture_finishes() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let resident = Resident {
            sender,
            busy: Arc::new(AtomicBool::new(false)),
        };
        resident.show().unwrap();
        receiver.try_recv().unwrap();
        resident.show().unwrap();
        assert!(receiver.try_recv().is_err());
        resident.busy.store(false, Ordering::Release);
        resident.show().unwrap();
        receiver.try_recv().unwrap();
    }

    #[test]
    fn disconnected_worker_allows_fallback() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let resident = Resident {
            sender,
            busy: Arc::new(AtomicBool::new(false)),
        };
        assert!(resident.show().is_err());
        assert!(resident.show().is_err());
    }
}
struct State {
    size: (i32, i32),
    screen: HDC,
    frozen: HDC,
    dimmed: HDC,
    start: Option<POINT>,
    current: POINT,
    cancelled: bool,
}
unsafe fn free_snapshot(dc: HDC) {
    if !dc.is_invalid() {
        unsafe {
            let bitmap = GetCurrentObject(dc, OBJ_BITMAP);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(bitmap);
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            free_snapshot(self.frozen);
            free_snapshot(self.dimmed);
            ReleaseDC(None, self.screen);
        }
    }
}
struct ClearState;
impl Drop for ClearState {
    fn drop(&mut self) {
        STATE.set(None);
    }
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
        if dc.is_invalid() || bitmap.is_invalid() {
            let _ = DeleteDC(dc);
            let _ = DeleteObject(bitmap.into());
            return Err(err("screen buffer allocation"));
        }
        SelectObject(dc, bitmap.into());
        if let Err(error) = BitBlt(
            dc,
            0,
            0,
            w,
            h,
            Some(screen),
            origin.x,
            origin.y,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        ) {
            free_snapshot(dc);
            return Err(format!("screen copy failed: {error}"));
        }
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
/// Device-independent bitmap in global memory: the clipboard owns it after
/// SetClipboardData, so it outlives this short-lived process.
unsafe fn copy_to_clipboard(h: HWND, s: &State, r: RECT) -> Result<(), String> {
    unsafe {
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        let dc = CreateCompatibleDC(Some(s.screen));
        let bitmap = CreateCompatibleBitmap(s.screen, w, hgt);
        let previous = SelectObject(dc, bitmap.into());
        let copied = BitBlt(dc, 0, 0, w, hgt, Some(s.frozen), r.left, r.top, SRCCOPY);
        SelectObject(dc, previous);
        if let Err(e) = copied {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            return Err(format!("selection copy failed: {e}"));
        }
        let header = std::mem::size_of::<BITMAPINFOHEADER>();
        let pixels = (w as usize) * 4 * (hgt as usize);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: header as u32,
                biWidth: w,
                biHeight: hgt,
                biPlanes: 1,
                biBitCount: 32,
                biSizeImage: pixels as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let global = GlobalAlloc(GMEM_MOVEABLE, header + pixels).map_err(|e| {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            format!("clipboard memory failed: {e}")
        })?;
        let base = GlobalLock(global).cast::<u8>();
        if base.is_null() {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            let _ = GlobalFree(Some(global));
            return Err("clipboard memory lock failed".into());
        }
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            hgt as u32,
            Some(base.add(header).cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        std::ptr::copy_nonoverlapping(
            (&info.bmiHeader as *const BITMAPINFOHEADER).cast::<u8>(),
            base,
            header,
        );
        let _ = GlobalUnlock(global);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        if lines != hgt {
            let _ = GlobalFree(Some(global));
            return Err(format!("pixel read failed: {lines} of {hgt} lines"));
        }
        OpenClipboard(Some(h)).map_err(|e| {
            let _ = GlobalFree(Some(global));
            format!("clipboard busy: {e}")
        })?;
        let result = EmptyClipboard().map_err(|e| e.to_string()).and_then(|()| {
            SetClipboardData(CF_DIB, Some(HANDLE(global.0)))
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(global));
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
enum Action {
    None,
    Capture,
    Release(RECT),
    Quit,
}
unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Win32 re-enters this procedure synchronously from calls such as
    // ReleaseCapture (WM_CAPTURECHANGED), so the state borrow must be
    // non-blocking and those calls happen after it is released.
    let outcome = STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let state = guard.as_mut()?;
        Some(match m {
            WM_PAINT => {
                unsafe { paint(h, state) };
                (true, Action::None)
            }
            WM_SETCURSOR => {
                unsafe { SetCursor(LoadCursorW(None, IDC_CROSS).ok()) };
                (true, Action::None)
            }
            WM_LBUTTONDOWN => {
                state.start = Some(point(l));
                state.current = point(l);
                let _ = unsafe { InvalidateRect(Some(h), None, false) };
                (true, Action::Capture)
            }
            WM_MOUSEMOVE if state.start.is_some() => {
                state.current = point(l);
                let _ = unsafe { InvalidateRect(Some(h), None, false) };
                (true, Action::None)
            }
            WM_LBUTTONUP if state.start.is_some() => {
                let start = state.start.take().unwrap_or(state.current);
                (true, Action::Release(selection(start, point(l))))
            }
            WM_RBUTTONDOWN | WM_KILLFOCUS => {
                state.cancelled = true;
                (true, Action::Quit)
            }
            WM_KEYDOWN if w.0 == VK_ESCAPE => {
                state.cancelled = true;
                (true, Action::Quit)
            }
            _ => (false, Action::None),
        })
    });
    let Some((handled, action)) = outcome else {
        return unsafe { DefWindowProcW(h, m, w, l) };
    };
    unsafe {
        match action {
            Action::None => {}
            Action::Capture => {
                SetCapture(h);
            }
            Action::Release(r) => {
                let _ = ReleaseCapture();
                let result = STATE.with_borrow(|state| {
                    state
                        .as_ref()
                        .map_or(Err("no capture state".to_owned()), |s| {
                            copy_to_clipboard(h, s, r)
                        })
                });
                if let Err(e) = result {
                    crate::log::write(&e);
                    STATE.with_borrow_mut(|state| {
                        if let Some(s) = state.as_mut() {
                            s.cancelled = true;
                        }
                    });
                }
                PostQuitMessage(0);
            }
            Action::Quit => PostQuitMessage(0),
        }
        if handled {
            LRESULT(0)
        } else {
            DefWindowProcW(h, m, w, l)
        }
    }
}
fn prepare() -> Result<(), String> {
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let class = wide("WinarchyShot");
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            ..Default::default()
        }) == 0
        {
            return Err(err("register screenshot window"));
        }
        // Initialize the thread's message queue without showing or capturing anything.
        let _ = PeekMessageW(&mut MSG::default(), None, 0, 0, PM_NOREMOVE);
    }
    Ok(())
}
pub fn run() -> Result<(), String> {
    prepare()?;
    if capture()? {
        std::process::exit(1);
    }
    Ok(())
}
fn capture() -> Result<bool, String> {
    let started = std::time::Instant::now();
    unsafe {
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
        let mut state = State {
            size,
            screen,
            frozen: HDC::default(),
            dimmed: HDC::default(),
            start: None,
            current: POINT::default(),
            cancelled: false,
        };
        state.frozen = snapshot(screen, origin, size, false)?;
        // Dim the frozen image, not a second potentially different desktop frame.
        state.dimmed = snapshot(state.frozen, POINT::default(), size, true)?;
        STATE.set(Some(state));
        let _clear = ClearState;
        let class = wide("WinarchyShot");
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
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
        crate::log::write(&format!(
            "shot: overlay ready in {} ms",
            started.elapsed().as_millis()
        ));
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result <= 0 {
                if result == -1 {
                    let _ = DestroyWindow(window);
                    return Err(err("screenshot message loop"));
                }
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = DestroyWindow(window);
        let cancelled = STATE.with_borrow_mut(|state| {
            let s = state.take();
            s.is_none_or(|s| s.cancelled)
        });
        Ok(cancelled)
    }
}
