use super::{Event, EventSender};
use crate::keyboard::Binding;
pub use crate::keyboard::parse;
use std::sync::{OnceLock, RwLock};
use windows::Win32::{
    Foundation::*,
    UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
static STATE: OnceLock<(EventSender, RwLock<Vec<Binding>>)> = OnceLock::new();
/// Set while a bar popup is shown so Escape is consumed and closes it instead
/// of reaching the foreground application.
pub static POPUP_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set while the keybindings editor records a chord: every key is consumed
/// and reported as `Event::Capture` instead of running its binding.
pub static CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CONSUMED: std::sync::Mutex<[bool; 256]> = std::sync::Mutex::new([false; 256]);
/// Virtual key plus one of the `dictate` binding currently held, 0 otherwise:
/// its repeats are swallowed and its release is reported instead of consumed.
static HELD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static MODIFIERS: std::sync::Mutex<crate::modifiers::Modifiers> =
    std::sync::Mutex::new(crate::modifiers::Modifiers::new());
fn resync_modifiers() {
    let mut state = MODIFIERS.lock().unwrap_or_else(|e| e.into_inner());
    for key in [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c] {
        state.update(key, unsafe { GetAsyncKeyState(key as i32) } < 0);
    }
}
pub fn update(bindings: Vec<Binding>) {
    if let Some((_, b)) = STATE.get() {
        *b.write().unwrap_or_else(|e| e.into_inner()) = bindings;
    }
}
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let k = *(l.0 as *const KBDLLHOOKSTRUCT);
            if k.vkCode < 256 && k.dwExtraInfo != super::native::INJECTED {
                let down = w.0 as u32 == WM_KEYDOWN || w.0 as u32 == WM_SYSKEYDOWN;
                let up = w.0 as u32 == WM_KEYUP || w.0 as u32 == WM_SYSKEYUP;
                let modifiers = {
                    let mut state = MODIFIERS.lock().unwrap_or_else(|e| e.into_inner());
                    if down || up {
                        state.update(k.vkCode, down);
                    }
                    state.mask() | u8::from(k.flags.0 & LLKHF_ALTDOWN.0 != 0)
                };
                if CAPTURE.load(std::sync::atomic::Ordering::Relaxed)
                    && let Some((tx, _)) = STATE.get()
                {
                    if down || up {
                        let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                        consumed[k.vkCode as usize] = false;
                        let _ = tx.send(Event::Capture(k.vkCode, modifiers, down));
                    }
                    return LRESULT(1);
                }
                if k.vkCode == 0x1b
                    && POPUP_OPEN.load(std::sync::atomic::Ordering::Relaxed)
                    && let Some((tx, _)) = STATE.get()
                {
                    if down {
                        let _ = tx.send(Event::Escape);
                    }
                    return LRESULT(1);
                }
                if up
                    && HELD.load(std::sync::atomic::Ordering::Relaxed) == k.vkCode + 1
                    && let Some((tx, _)) = STATE.get()
                {
                    HELD.store(0, std::sync::atomic::Ordering::Relaxed);
                    let _ = tx.send(Event::Dictate(false));
                    return LRESULT(1);
                }
                if up {
                    let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                    if consumed[k.vkCode as usize] {
                        consumed[k.vkCode as usize] = false;
                        return LRESULT(1);
                    }
                }
                if down
                    && let Some((tx, bindings)) = STATE.get()
                    && let Some(b) = bindings
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .iter()
                        .find(|b| b.key == k.vkCode && b.modifiers == modifiers)
                {
                    if b.command == crate::command::Command::Dictate {
                        // Auto-repeat keeps sending key-down while the key is held.
                        if HELD.swap(k.vkCode + 1, std::sync::atomic::Ordering::Relaxed)
                            != k.vkCode + 1
                        {
                            let _ = tx.send(Event::Dictate(true));
                        }
                        return LRESULT(1);
                    }
                    let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                    if !consumed[k.vkCode as usize]
                        && tx.send(Event::Command(b.command.clone(), None)).is_err()
                    {
                        drop(consumed);
                        return CallNextHookEx(None, code, w, l);
                    }
                    consumed[k.vkCode as usize] = true;
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, w, l)
    }
}
unsafe extern "system" fn window_event(
    _: HWINEVENTHOOK,
    event: u32,
    h: HWND,
    object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        resync_modifiers();
    }
    if object == 0
        && !h.is_invalid()
        && let Some((tx, _)) = STATE.get()
    {
        let _ = tx.send(Event::Window(event, h.0 as isize));
    }
}
unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && w.0 as u32 == WM_MOUSEMOVE {
            static LAST: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
            let event = *(l.0 as *const MSLLHOOKSTRUCT);
            let h = GetAncestor(WindowFromPoint(event.pt), GA_ROOT).0 as isize;
            if LAST.swap(h, std::sync::atomic::Ordering::Relaxed) != h
                && let Some((tx, _)) = STATE.get()
            {
                let _ = tx.send(Event::Mouse(h));
            }
        }
        if code >= 0
            && w.0 as u32 == WM_LBUTTONDOWN
            && let Some((tx, _)) = STATE.get()
        {
            let event = *(l.0 as *const MSLLHOOKSTRUCT);
            let h = GetAncestor(WindowFromPoint(event.pt), GA_ROOT).0 as isize;
            let _ = tx.send(Event::Click(h));
        }
        CallNextHookEx(None, code, w, l)
    }
}
unsafe extern "system" fn display_window(h: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if (message == WM_DISPLAYCHANGE || message == WM_SETTINGCHANGE)
        && let Some((tx, _)) = STATE.get()
    {
        let _ = tx.send(Event::Display);
    }
    unsafe { DefWindowProcW(h, message, w, l) }
}
pub fn start(tx: EventSender, bindings: Vec<Binding>) -> Result<(), String> {
    let _ = STATE.set((tx, RwLock::new(bindings)));
    let (ready, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        resync_modifiers();
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0);
        match hook {
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
            }
            Ok(hook) => {
                let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0).ok();
                let hooks = [
                    SetWinEventHook(
                        EVENT_OBJECT_CREATE,
                        EVENT_OBJECT_HIDE,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    ),
                    // Firefox shows new windows DWM-cloaked until their first
                    // paint; the uncloak is the first moment they are enrollable.
                    SetWinEventHook(
                        EVENT_OBJECT_UNCLOAKED,
                        EVENT_OBJECT_UNCLOAKED,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    ),
                    SetWinEventHook(
                        EVENT_SYSTEM_FOREGROUND,
                        EVENT_SYSTEM_FOREGROUND,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    ),
                    SetWinEventHook(
                        EVENT_SYSTEM_MINIMIZESTART,
                        EVENT_SYSTEM_MINIMIZEEND,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    ),
                    SetWinEventHook(
                        EVENT_SYSTEM_MOVESIZEEND,
                        EVENT_SYSTEM_MOVESIZEEND,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    ),
                ];
                let class = super::native::wide("WinarchyEvents");
                let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                    .unwrap_or_default();
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(display_window),
                    hInstance: instance.into(),
                    lpszClassName: windows::core::PCWSTR(class.as_ptr()),
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let _display = CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    windows::core::PCWSTR(class.as_ptr()),
                    windows::core::PCWSTR(class.as_ptr()),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                );
                if hooks.iter().any(|h| h.is_invalid()) || _display.is_err() {
                    let _ = ready.send(Err("window event initialization failed".into()));
                    let _ = UnhookWindowsHookEx(hook);
                    for h in hooks {
                        let _ = UnhookWinEvent(h);
                    }
                    return;
                }
                let _ = ready.send(Ok(()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
                if let Some(h) = mouse_hook {
                    let _ = UnhookWindowsHookEx(h);
                }
                for h in hooks {
                    let _ = UnhookWinEvent(h);
                }
            }
        }
    });
    rx.recv().map_err(|e| e.to_string())?
}
