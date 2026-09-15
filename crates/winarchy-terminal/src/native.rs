//! The window procedure never borrows application state: it only validates
//! paint regions and posts notifications. Win32's synchronous re-entrancy
//! therefore cannot alias a mutable terminal/renderer borrow.
use alacritty_terminal::{
    grid::Scroll,
    index::Side,
    selection::{Selection, SelectionType},
    term::TermMode,
};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender},
    },
    time::Instant,
};
use winarchy_terminal::{
    config::Config,
    input::{self, Mods},
    palette::Palette,
    reload::{self, Snapshot},
    render::{Fonts, Frame, Graphics, Surface},
    session::Session,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, DataExchange::*, LibraryLoader::*, Memory::*, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
pub const REQUEST: u32 = WM_APP + 1;
const OUTPUT: u32 = WM_APP + 2;
const PAINT: u32 = WM_APP + 3;
const RESIZE: u32 = WM_APP + 4;
const SPARE: u32 = WM_APP + 5;
const CLOSE: u32 = WM_APP + 6;
const RELOAD: u32 = WM_APP + 7;
const CLASS: PCWSTR = w!("WinarchyTerminal");
pub struct Request {
    pub command: String,
    pub reply: SyncSender<Result<String, String>>,
}
struct Window {
    hwnd: HWND,
    id: usize,
    surface: Surface,
    session: Option<Session>,
    pending: Arc<AtomicBool>,
    timer: bool,
    selecting: bool,
    mouse_button: Option<u8>,
    last_mouse: Option<(usize, usize)>,
    surrogate: Option<u16>,
    config: Config,
    startup_trace: Option<StartupTrace>,
}
// Opt-in, bounded metadata only: no virtual-key values, characters or PTY text.
struct StartupTrace {
    started: Instant,
    paints: u8,
    inputs: u8,
}
struct App {
    windows: HashMap<usize, Window>,
    spare: Option<HWND>,
    next_id: usize,
    graphics: Rc<Graphics>,
    fonts: Rc<Fonts>,
    snapshot: Snapshot,
    config: Config,
    palette: Palette,
    thread: u32,
    resident: bool,
}
fn post(thread: u32, message: u32, w: usize, l: isize) {
    unsafe {
        let _ = PostThreadMessageW(thread, message, WPARAM(w), LPARAM(l));
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                post(GetCurrentThreadId(), PAINT, hwnd.0 as usize, 0);
                LRESULT(0)
            }
            WM_SIZE => {
                post(GetCurrentThreadId(), RESIZE, hwnd.0 as usize, 0);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let r = &*(l.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                post(GetCurrentThreadId(), RESIZE, hwnd.0 as usize, 0);
                LRESULT(0)
            }
            WM_CLOSE => {
                post(GetCurrentThreadId(), CLOSE, hwnd.0 as usize, 0);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let m = &mut *(l.0 as *mut MINMAXINFO);
                m.ptMinTrackSize = POINT { x: 160, y: 100 };
                LRESULT(0)
            }
            // Retain native resize hit-testing but remove the title-bar area.
            WM_NCCALCSIZE if w.0 != 0 => LRESULT(0),
            WM_NCHITTEST => {
                let hit = DefWindowProcW(hwnd, message, w, l);
                if hit.0 == HTCAPTION as isize {
                    LRESULT(HTCLIENT as isize)
                } else {
                    hit
                }
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}
pub fn run(
    resident: bool,
    open: bool,
    requests: Option<Receiver<Request>>,
    ready: impl FnOnce(u32),
) -> Result<(), String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let home = winarchy_theme::config_home();
    let config = Config::load(&home).unwrap_or_else(|e| {
        crate::log(&e);
        Config::default()
    });
    let theme = winarchy_theme::Theme::current(&home);
    let palette = Palette::new(&theme);
    let start = Instant::now();
    let graphics = Graphics::new(&config).map_err(|e| e.to_string())?;
    let thread = unsafe { GetCurrentThreadId() };
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: GetModuleHandleW(None).map_err(|e| e.to_string())?.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_IBEAM).map_err(|e| e.to_string())?,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_win32().to_string());
        }
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
    }
    let mut app = App {
        windows: HashMap::new(),
        spare: None,
        next_id: 1,
        fonts: graphics.fonts.clone(),
        snapshot: Snapshot {
            config: config.clone(),
            theme,
        },
        graphics,
        config,
        palette,
        thread,
        resident,
    };
    app.prepare()?;
    let reloaded = reload::watch(home, move || post(thread, RELOAD, 0, 0))
        .map_err(|e| crate::log(&format!("theme watcher: {e}")))
        .ok();
    crate::log(&format!(
        "resident_ready_ms={:.2}",
        start.elapsed().as_secs_f64() * 1000.
    ));
    ready(thread);
    if open {
        app.open()?;
    }
    unsafe {
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result == 0 {
                break;
            }
            if result < 0 {
                return Err(windows::core::Error::from_win32().to_string());
            }
            if msg.hwnd.0.is_null() {
                match msg.message {
                    REQUEST => {
                        if let Some(rx) = &requests {
                            while let Ok(request) = rx.try_recv() {
                                let result = match request.command.as_str() {
                                    "open" => app.open().map(|_| "opened".into()),
                                    "pid" => Ok(std::process::id().to_string()),
                                    "status" => Ok(format!(
                                        "windows={} spare={} pid={}",
                                        app.windows
                                            .values()
                                            .filter(|w| w.session.is_some())
                                            .count(),
                                        app.spare.is_some(),
                                        std::process::id()
                                    )),
                                    "quit" => {
                                        if app.windows.values().any(|w| w.session.is_some()) {
                                            Err("Close terminal windows before stopping the resident".into())
                                        } else {
                                            PostQuitMessage(0);
                                            Ok("stopping".into())
                                        }
                                    }
                                    _ => Err("expected open, pid, status or quit".into()),
                                };
                                let _ = request.reply.send(result);
                            }
                        }
                    }
                    OUTPUT => {
                        if let Some(window) =
                            app.windows.values_mut().find(|w| w.id == msg.wParam.0)
                        {
                            window.schedule();
                        }
                    }
                    PAINT => {
                        if let Some(window) = app.windows.get_mut(&msg.wParam.0) {
                            window.paint(&app.palette);
                        }
                    }
                    RESIZE => {
                        if let Some(window) = app.windows.get_mut(&msg.wParam.0) {
                            window.resize();
                        }
                    }
                    CLOSE => app.close(msg.wParam.0),
                    RELOAD => {
                        if let Some(slot) = &reloaded
                            && let Some(result) = slot.lock().unwrap().take()
                        {
                            match result {
                                Ok(snapshot) => app.apply(snapshot),
                                Err(e) => {
                                    crate::log(&format!("reload retained last valid settings: {e}"))
                                }
                            }
                        }
                    }
                    SPARE => {
                        if let Err(e) = app.prepare() {
                            crate::log(&format!("prepare window: {e}"));
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(window) = app.windows.get_mut(&(msg.hwnd.0 as usize))
                && window.message(&msg, &app.palette, &app.graphics)
            {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for (_, window) in app.windows.drain() {
            let _ = DestroyWindow(window.hwnd);
            drop(window);
        }
        CoUninitialize();
    }
    Ok(())
}
impl App {
    fn apply(&mut self, snapshot: Snapshot) {
        if snapshot == self.snapshot {
            return;
        }
        let fonts_changed = snapshot.config.font_family != self.config.font_family
            || snapshot.config.font_size != self.config.font_size;
        let fonts = if fonts_changed {
            match self.graphics.fonts(&snapshot.config) {
                Ok(f) => f,
                Err(e) => {
                    crate::log(&format!("font reload: {e}"));
                    return;
                }
            }
        } else {
            self.fonts.clone()
        };
        self.palette = Palette::new(&snapshot.theme);
        self.config = snapshot.config.clone();
        self.fonts = fonts;
        for window in self.windows.values_mut() {
            if fonts_changed {
                window.surface.fonts = self.fonts.clone();
                window.config.font_family = self.config.font_family.clone();
                window.config.font_size = self.config.font_size;
            }
            window.surface.padding = self.config.padding as f32;
            if let Some(session) = &window.session {
                *session.palette.lock().unwrap() = self.palette.clone();
                if snapshot.config.scrollback != self.snapshot.config.scrollback {
                    session.model.lock().unwrap().term.set_options(
                        alacritty_terminal::term::Config {
                            scrolling_history: snapshot.config.scrollback,
                            osc52: alacritty_terminal::term::Osc52::Disabled,
                            ..Default::default()
                        },
                    );
                }
                window.resize();
                window.paint(&self.palette);
            } else {
                window.config = self.config.clone();
                if let Err(e) = window.surface.draw(&Frame::new(None, &self.palette)) {
                    crate::log(&e.to_string());
                }
            }
        }
        self.snapshot = snapshot;
    }
    fn prepare(&mut self) -> Result<(), String> {
        if self.spare.is_some() || self.windows.len() >= 16 {
            return Ok(());
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP,
                CLASS,
                w!("Winarchy Terminal"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1000,
                650,
                None,
                None,
                Some(GetModuleHandleW(None).map_err(|e| e.to_string())?.into()),
                None,
            )
        }
        .map_err(|e| e.to_string())?;
        let result = (|| {
            let mut area = RECT::default();
            unsafe { GetClientRect(hwnd, &mut area) }.map_err(|e| e.to_string())?;
            let mut surface = Surface::new(
                self.graphics.clone(),
                hwnd,
                (area.right - area.left) as u32,
                (area.bottom - area.top) as u32,
                unsafe { GetDpiForWindow(hwnd) },
                self.config.padding,
            )
            .map_err(|e| e.to_string())?;
            surface.fonts = self.fonts.clone();
            surface
                .draw(&Frame::new(None, &self.palette))
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(surface)
        })();
        let surface = match result {
            Ok(s) => s,
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        self.windows.insert(
            hwnd.0 as usize,
            Window {
                hwnd,
                id: self.next_id,
                surface,
                session: None,
                pending: Arc::new(AtomicBool::new(false)),
                timer: false,
                selecting: false,
                mouse_button: None,
                last_mouse: None,
                surrogate: None,
                config: self.config.clone(),
                startup_trace: None,
            },
        );
        self.next_id += 1;
        self.spare = Some(hwnd);
        Ok(())
    }
    fn open(&mut self) -> Result<(), String> {
        let at = Instant::now();
        self.prepare()?;
        let hwnd = self
            .spare
            .take()
            .ok_or("Maximum of 16 terminal windows reached")?;
        let window = self.windows.get_mut(&(hwnd.0 as usize)).unwrap();
        if std::env::var_os("WINARCHY_TERMINAL_TRACE_STARTUP").is_some() {
            window.startup_trace = Some(StartupTrace {
                started: at,
                paints: 0,
                inputs: 0,
            });
            title(hwnd, "Winarchy Terminal — diagnostic");
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        crate::log(&format!(
            "show_ms={:.2} id={}",
            at.elapsed().as_secs_f64() * 1000.,
            window.id
        ));
        let thread = self.thread;
        let id = window.id;
        let pending = window.pending.clone();
        let wake = Arc::new(move || {
            if !pending.swap(true, Ordering::AcqRel) {
                post(thread, OUTPUT, id, 0);
            }
        });
        // The themed frame already exists and the window is visible BEFORE any
        // PTY/model construction or WSL process launch.
        window.session = Some(Session::start(
            window.config.clone(),
            window.surface.size(),
            self.palette.clone(),
            wake,
        ));
        if self.resident {
            post(self.thread, SPARE, 0, 0);
        }
        Ok(())
    }
    fn close(&mut self, key: usize) {
        if let Some(window) = self.windows.remove(&key) {
            if self.spare == Some(window.hwnd) {
                self.spare = None;
            }
            unsafe {
                let _ = KillTimer(Some(window.hwnd), 1);
                let _ = DestroyWindow(window.hwnd);
            }
            window.surface.trim();
            drop(window);
        }
        if self.resident {
            post(self.thread, SPARE, 0, 0);
        } else if self.windows.is_empty() {
            unsafe {
                PostQuitMessage(0);
            }
        }
    }
}
impl Window {
    fn schedule(&mut self) {
        unsafe {
            if !IsWindowVisible(self.hwnd).as_bool() || IsIconic(self.hwnd).as_bool() {
                self.pending.store(false, Ordering::Release);
                return;
            }
            if !self.timer {
                self.timer = SetTimer(Some(self.hwnd), 1, 8, None) != 0;
            }
        }
    }
    fn paint(&mut self, palette: &Palette) {
        self.pending.store(false, Ordering::Release);
        if unsafe { !IsWindowVisible(self.hwnd).as_bool() || IsIconic(self.hwnd).as_bool() } {
            return;
        }
        let sync_pending = self.session.as_ref().is_some_and(Session::expire_sync);
        let frame = if let Some(session) = &self.session {
            let m = session.model.lock().unwrap();
            if let Some(e) = &m.error {
                title(self.hwnd, &format!("Winarchy Terminal — {e}"));
            } else if m.exited {
                title(
                    self.hwnd,
                    "Winarchy Terminal — WSL exited (close this window)",
                );
            }
            Frame::new(Some(&m), palette)
        } else {
            Frame::new(None, palette)
        };
        if sync_pending {
            self.schedule();
        }
        let editor_ready = self.startup_trace.as_ref().is_some_and(|t| t.paints < 8)
            && self.mode().contains(TermMode::BRACKETED_PASTE);
        if let Some(trace) = &mut self.startup_trace
            && trace.paints < 8
        {
            trace.paints += 1;
            crate::log(&format!(
                "startup id={} ms={:.2} paint={} editor_ready={} sync_pending={}",
                self.id,
                trace.started.elapsed().as_secs_f64() * 1000.,
                trace.paints,
                editor_ready,
                sync_pending,
            ));
        }
        if let Err(e) = self.surface.draw(&frame) {
            crate::log(&format!("render: {e}"));
            title(
                self.hwnd,
                "Winarchy Terminal — rendering failed; reopen window",
            );
        }
    }
    fn resize(&mut self) {
        let mut r = RECT::default();
        unsafe {
            if IsIconic(self.hwnd).as_bool() {
                return;
            }
            let _ = GetClientRect(self.hwnd, &mut r);
        }
        if let Err(e) = self
            .surface
            .resize(r.right.max(1) as u32, r.bottom.max(1) as u32, unsafe {
                GetDpiForWindow(self.hwnd)
            })
        {
            crate::log(&format!("resize: {e}"));
            return;
        }
        if let Some(s) = &self.session {
            s.resize(self.surface.size());
        }
        self.schedule();
    }
    fn send(&self, bytes: Vec<u8>) {
        if let Some(s) = &self.session
            && let Err(e) = s.send(bytes)
        {
            title(self.hwnd, e);
            crate::log(e);
            s.model.lock().unwrap().error = Some(e.into());
        }
    }
    fn mode(&self) -> TermMode {
        self.session
            .as_ref()
            .map(|s| *s.model.lock().unwrap().term.mode())
            .unwrap_or_default()
    }
    fn message(&mut self, msg: &MSG, palette: &Palette, g: &Rc<Graphics>) -> bool {
        let m = mods();
        let mode = self.mode();
        if matches!(
            msg.message,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR
        ) && let Some(trace) = &mut self.startup_trace
            && trace.inputs < 12
        {
            trace.inputs += 1;
            crate::log(&format!(
                "startup id={} ms={:.2} input_event={} message={:#x} ctrl={} alt={} shift={} editor_ready={} pending={} timer={}",
                self.id,
                trace.started.elapsed().as_secs_f64() * 1000.,
                trace.inputs,
                msg.message,
                m.ctrl,
                m.alt,
                m.shift,
                mode.contains(TermMode::BRACKETED_PASTE),
                self.pending.load(Ordering::Acquire),
                self.timer,
            ));
        }
        match msg.message {
            WM_TIMER if msg.wParam.0 == 1 => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), 1);
                }
                self.timer = false;
                self.paint(palette);
                true
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = msg.wParam.0 as u16;
                // Let TranslateMessage compose AltGr characters such as @,#,{.
                if m.alt_gr(pressed(VK_RMENU.0)) {
                    return false;
                }
                if vk == 13 && m.alt {
                    return true;
                }
                if m.ctrl && m.shift && vk == 0x43 {
                    if let Some(s) = &self.session
                        && let Some(text) = s.model.lock().unwrap().term.selection_to_string()
                        && let Err(e) = clipboard_set(self.hwnd, &text)
                    {
                        crate::log(&e);
                    }
                    return true;
                }
                if (m.ctrl && m.shift && vk == 0x56) || (m.shift && vk == 0x2d) {
                    if let Ok(text) = clipboard_get(self.hwnd) {
                        self.send(input::paste(
                            &text,
                            mode.contains(TermMode::BRACKETED_PASTE),
                        ));
                    }
                    return true;
                }
                if m.ctrl && matches!(vk, 0xbb | 0xbd | 0x30 | 0x6b | 0x6d) {
                    self.config.font_size = if vk == 0x30 {
                        Config::load(&winarchy_theme::config_home())
                            .unwrap_or_default()
                            .font_size
                    } else {
                        (self.config.font_size + if vk == 0xbd || vk == 0x6d { -1. } else { 1. })
                            .clamp(6., 72.)
                    };
                    if let Ok(fonts) = g.fonts(&self.config) {
                        self.surface.fonts = fonts;
                        self.resize();
                    }
                    return true;
                }
                if m.shift && matches!(vk, 0x21 | 0x22) {
                    if let Some(s) = &self.session {
                        s.model.lock().unwrap().term.scroll_display(if vk == 0x21 {
                            Scroll::PageUp
                        } else {
                            Scroll::PageDown
                        });
                        self.schedule();
                    }
                    return true;
                }
                if let Some(bytes) = input::key(vk, m, mode) {
                    self.send(bytes);
                    return true;
                }
                false
            }
            WM_CHAR | WM_SYSCHAR => {
                let unit = msg.wParam.0 as u16;
                if (0xd800..=0xdbff).contains(&unit) {
                    self.surrogate = Some(unit);
                    return true;
                }
                let c = if (0xdc00..=0xdfff).contains(&unit) {
                    self.surrogate.take().and_then(|high| {
                        char::from_u32(
                            0x10000 + (((high as u32 - 0xd800) << 10) | (unit as u32 - 0xdc00)),
                        )
                    })
                } else {
                    self.surrogate = None;
                    char::from_u32(unit as u32)
                };
                if let Some(c) = c {
                    let mut bytes = Vec::new();
                    // Right Alt + Ctrl is AltGr: never prefix its composed text.
                    if m.alt && !m.alt_gr(pressed(VK_RMENU.0)) {
                        bytes.push(27);
                    }
                    bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                    for _ in 0..(msg.lParam.0 as u16).max(1) {
                        self.send(bytes.clone());
                    }
                    if let Some(s) = &self.session {
                        let mut model = s.model.lock().unwrap();
                        model.term.selection = None;
                        model.term.scroll_display(Scroll::Bottom);
                    }
                    self.schedule();
                }
                true
            }
            WM_SETFOCUS | WM_KILLFOCUS => {
                if mode.contains(TermMode::FOCUS_IN_OUT) {
                    self.send(
                        if msg.message == WM_SETFOCUS {
                            b"\x1b[I"
                        } else {
                            b"\x1b[O"
                        }
                        .to_vec(),
                    );
                }
                false
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_RBUTTONDOWN
            | WM_RBUTTONUP | WM_MOUSEMOVE | WM_MOUSEWHEEL => {
                self.mouse(msg, m, mode);
                true
            }
            _ => false,
        }
    }
    fn mouse(&mut self, msg: &MSG, m: Mods, mode: TermMode) {
        let mut p = POINT {
            x: msg.lParam.0 as i16 as i32,
            y: (msg.lParam.0 >> 16) as i16 as i32,
        };
        if msg.message == WM_MOUSEWHEEL {
            unsafe {
                let _ = ScreenToClient(self.hwnd, &mut p);
            }
        }
        let (col, row) = self.surface.cell_at(p.x, p.y);
        let reporting = mode.intersects(TermMode::MOUSE_MODE) && !m.shift;
        let release = matches!(msg.message, WM_LBUTTONUP | WM_MBUTTONUP | WM_RBUTTONUP);
        if msg.message == WM_MOUSEWHEEL {
            let delta = (msg.wParam.0 >> 16) as i16;
            let count = (delta.unsigned_abs() as usize / 120).max(1);
            if reporting {
                for _ in 0..count {
                    if let Some(bytes) =
                        input::mouse(if delta > 0 { 64 } else { 65 }, col, row, false, m, mode)
                    {
                        self.send(bytes);
                    }
                }
            } else if let Some(s) = &self.session {
                s.model
                    .lock()
                    .unwrap()
                    .term
                    .scroll_display(Scroll::Delta(if delta > 0 {
                        3 * count as i32
                    } else {
                        -3 * count as i32
                    }));
                self.schedule();
            }
            return;
        }
        if msg.message == WM_MOUSEMOVE {
            if self.last_mouse == Some((col, row)) {
                return;
            }
            self.last_mouse = Some((col, row));
            if reporting
                && (mode.contains(TermMode::MOUSE_MOTION)
                    || (mode.contains(TermMode::MOUSE_DRAG) && self.mouse_button.is_some()))
            {
                if let Some(bytes) = input::mouse(
                    32 + self.mouse_button.unwrap_or(3),
                    col,
                    row,
                    false,
                    m,
                    mode,
                ) {
                    self.send(bytes);
                }
            } else if self.selecting
                && let Some(s) = &self.session
            {
                let mut model = s.model.lock().unwrap();
                let point = model.point(col, row);
                if let Some(selection) = &mut model.term.selection {
                    selection.update(point, Side::Right);
                }
                drop(model);
                self.schedule();
            }
            return;
        }
        let button = match msg.message {
            WM_LBUTTONDOWN | WM_LBUTTONUP => 0,
            WM_MBUTTONDOWN | WM_MBUTTONUP => 1,
            _ => 2,
        };
        unsafe {
            if release {
                let _ = ReleaseCapture();
            } else {
                SetCapture(self.hwnd);
            }
        }
        self.mouse_button = if release { None } else { Some(button) };
        if reporting {
            if let Some(bytes) = input::mouse(button, col, row, release, m, mode) {
                self.send(bytes);
            }
        } else if button == 0 {
            self.selecting = !release;
            if !release && let Some(s) = &self.session {
                let mut model = s.model.lock().unwrap();
                model.term.selection = Some(Selection::new(
                    SelectionType::Simple,
                    model.point(col, row),
                    Side::Left,
                ));
            }
            self.schedule();
        }
    }
}
fn pressed(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}
fn mods() -> Mods {
    Mods {
        shift: pressed(VK_SHIFT.0),
        ctrl: pressed(VK_CONTROL.0),
        alt: pressed(VK_MENU.0),
    }
}
fn title(hwnd: HWND, text: &str) {
    let text: Vec<u16> = text.encode_utf16().take(512).chain([0]).collect();
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(text.as_ptr()));
    }
}
struct Clipboard;
impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
fn clipboard_get(hwnd: HWND) -> Result<String, String> {
    unsafe {
        OpenClipboard(Some(hwnd)).map_err(|e| e.to_string())?;
        let _guard = Clipboard;
        let handle = GetClipboardData(13).map_err(|e| e.to_string())?;
        let size = GlobalSize(HGLOBAL(handle.0));
        if size > 128 * 1024 {
            return Err("Clipboard exceeds 64K UTF-16 units".into());
        }
        let ptr = GlobalLock(HGLOBAL(handle.0));
        if ptr.is_null() {
            return Err("Unable to lock clipboard".into());
        }
        let units = std::slice::from_raw_parts(ptr.cast::<u16>(), size / 2);
        let len = units.iter().position(|c| *c == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..len]);
        let _ = GlobalUnlock(HGLOBAL(handle.0));
        Ok(text)
    }
}
fn clipboard_set(hwnd: HWND, text: &str) -> Result<(), String> {
    unsafe {
        OpenClipboard(Some(hwnd)).map_err(|e| e.to_string())?;
        let _guard = Clipboard;
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let handle = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err("Unable to allocate clipboard".into());
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), ptr.cast(), units.len());
        let _ = GlobalUnlock(handle);
        if let Err(e) =
            EmptyClipboard().and_then(|_| SetClipboardData(13, Some(HANDLE(handle.0))).map(|_| ()))
        {
            let _ = GlobalFree(Some(handle));
            return Err(e.to_string());
        }
        Ok(())
    }
}
