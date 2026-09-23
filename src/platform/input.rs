#[cfg(test)]
#[path = "input_popup_tests.rs"]
mod popup_tests;

#[path = "input_raw.rs"]
mod raw;

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
static POPUP_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static HOOK_THREAD: OnceLock<u32> = OnceLock::new();
static TRACE_ESCAPE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Opt-in, Escape-only diagnostics. No other key or typed text is recorded.
#[derive(Debug)]
pub struct EscapeTrace {
    pub source: &'static str,
    pub down: bool,
    pub popup: bool,
    pub hints: u32,
    pub capture: bool,
    pub consumed: bool,
    pub flags: u32,
    pub extra: usize,
}

const REFRESH_KEYBOARD_HOOK: u32 = WM_APP + 1;

/// Claim Escape when a popup opens, without activating its window. A foreground
/// application may have installed a newer low-level hook since daemon startup;
/// Windows calls newer hooks first, and a consumed key never reaches ours.
pub fn set_popup_open(open: bool) {
    let was_open = POPUP_OPEN.swap(open, std::sync::atomic::Ordering::Relaxed);
    if open != was_open && TRACE_ESCAPE.load(std::sync::atomic::Ordering::Relaxed) {
        tracing::info!(open, "Escape diagnostic: popup capture changed");
    }
    if open
        && !was_open
        && let Some(thread) = HOOK_THREAD.get()
    {
        // Hooks must be replaced on their message-pumping thread, not the UI
        // thread (which can spend time compiling an applet).
        if let Err(error) =
            unsafe { PostThreadMessageW(*thread, REFRESH_KEYBOARD_HOOK, WPARAM(0), LPARAM(0)) }
        {
            tracing::warn!(%error, "could not refresh popup keyboard capture");
        }
    }
}

/// Nonzero opening generation while bar hints consume keyboard input.
pub static BAR_HINTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Set while the keybindings editor records a chord: every key is consumed
/// and reported as `Event::Capture` instead of running its binding.
pub static CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set while the lock screen is shown: no binding runs and navigation keys
/// are swallowed (`lockscreen::blocked`).
pub static LOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The password surface is the foreground window. Until then every key is
/// swallowed, so a password can never land in the application behind.
pub static LOCK_FOCUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Consumed {
    No,
    Binding,
    Modal,
}
static CONSUMED: std::sync::Mutex<[Consumed; 256]> = std::sync::Mutex::new([Consumed::No; 256]);
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
            if k.vkCode == VK_ESCAPE.0 as u32
                && TRACE_ESCAPE.load(std::sync::atomic::Ordering::Relaxed)
                && let Some((tx, _)) = STATE.get()
            {
                // Never log or query the foreground window on the hook thread.
                let consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner())
                    [k.vkCode as usize]
                    != Consumed::No;
                let _ = tx.send(Event::EscapeTrace(EscapeTrace {
                    source: "hook",
                    down: w.0 as u32 == WM_KEYDOWN || w.0 as u32 == WM_SYSKEYDOWN,
                    popup: POPUP_OPEN.load(std::sync::atomic::Ordering::Relaxed),
                    hints: BAR_HINTS.load(std::sync::atomic::Ordering::Relaxed),
                    capture: CAPTURE.load(std::sync::atomic::Ordering::Relaxed),
                    consumed,
                    flags: k.flags.0,
                    extra: k.dwExtraInfo,
                }));
            }
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
                        consumed[k.vkCode as usize] = Consumed::No;
                        let _ = tx.send(Event::Capture(k.vkCode, modifiers, down));
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
                    if consumed[k.vkCode as usize] != Consumed::No {
                        consumed[k.vkCode as usize] = Consumed::No;
                        return LRESULT(1);
                    }
                }
                if LOCKED.load(std::sync::atomic::Ordering::Relaxed) {
                    if down
                        && (!LOCK_FOCUSED.load(std::sync::atomic::Ordering::Relaxed)
                            || crate::lockscreen::blocked(k.vkCode, modifiers))
                    {
                        CONSUMED.lock().unwrap_or_else(|e| e.into_inner())[k.vkCode as usize] =
                            Consumed::Modal;
                        return LRESULT(1);
                    }
                    return CallNextHookEx(None, code, w, l);
                }
                if down && let Some((tx, bindings)) = STATE.get() {
                    let bindings = bindings.read().unwrap_or_else(|e| e.into_inner());
                    let binding = bindings
                        .iter()
                        .find(|b| b.key == k.vkCode && b.modifiers == modifiers);
                    let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                    // A hint selection must swallow repeats until release, even
                    // after selecting has already closed the mode.
                    if consumed[k.vkCode as usize] == Consumed::Modal
                        || (consumed[k.vkCode as usize] == Consumed::Binding
                            && !binding.is_some_and(|b| {
                                matches!(b.command, crate::command::Command::Resize { .. })
                            }))
                    {
                        return LRESULT(1);
                    }
                    let epoch = BAR_HINTS.load(std::sync::atomic::Ordering::Relaxed);
                    // Escape is global, not generation-tagged selection input:
                    // queued just after selecting (or during view compilation),
                    // it must still close the newly opened applet.
                    if k.vkCode == 0x1b
                        && (epoch != 0 || POPUP_OPEN.load(std::sync::atomic::Ordering::Relaxed))
                    {
                        consumed[k.vkCode as usize] = Consumed::Modal;
                        let _ = tx.send(Event::Escape);
                        return LRESULT(1);
                    }
                    if epoch != 0 {
                        consumed[k.vkCode as usize] = Consumed::Modal;
                        // New modifier presses must not open Start/menus in the
                        // previous app. Releases of modifiers held before entering
                        // still pass through, so Ctrl/Alt cannot become stuck.
                        if crate::keyboard::is_modifier(k.vkCode) {
                            return LRESULT(1);
                        }
                        if binding.is_some_and(|b| b.command == crate::command::Command::BarHints) {
                            let _ =
                                tx.send(Event::Command(crate::command::Command::BarHints, None));
                        } else if modifiers & !crate::keyboard::SHIFT == 0
                            && let Some(input) = crate::bar_hints::input(k.vkCode)
                        {
                            let _ = tx.send(Event::BarHintKey(epoch, input));
                        }
                        // Invalid keys do not type into the previous application.
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
                    // Only resize repeats: toggles and other actions stay one-shot.
                    if (consumed[k.vkCode as usize] == Consumed::No
                        || matches!(b.command, crate::command::Command::Resize { .. }))
                        && tx.send(Event::Command(b.command.clone(), None)).is_err()
                    {
                        drop(consumed);
                        return CallNextHookEx(None, code, w, l);
                    }
                    consumed[k.vkCode as usize] = Consumed::Binding;
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
    if message == WM_INPUT {
        raw::received(l);
        // DefWindowProc must still run to release foreground raw-input storage.
    }
    if (message == WM_DISPLAYCHANGE || message == WM_SETTINGCHANGE)
        && let Some((tx, _)) = STATE.get()
    {
        let _ = tx.send(Event::Display);
    }
    unsafe { DefWindowProcW(h, message, w, l) }
}
pub fn start(tx: EventSender, bindings: Vec<Binding>) -> Result<(), String> {
    TRACE_ESCAPE.store(
        std::env::var("WINARCHY_TRACE_ESCAPE").as_deref() == Ok("1"),
        std::sync::atomic::Ordering::Relaxed,
    );
    let _ = STATE.set((tx, RwLock::new(bindings)));
    let (ready, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        resync_modifiers();
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0);
        match hook {
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
            }
            Ok(mut hook) => {
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
                if let Ok(window) = _display
                    && let Err(error) = raw::register(window)
                {
                    tracing::warn!(%error, "physical popup Escape capture unavailable");
                }
                let _ = HOOK_THREAD.set(windows::Win32::System::Threading::GetCurrentThreadId());
                let _ = ready.send(Ok(()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.hwnd.is_invalid() && msg.message == REFRESH_KEYBOARD_HOOK {
                        // Install first: retain the working hook if replacement
                        // fails. Keep consumed-key state so Escape repeats and
                        // its eventual release cannot leak into the client.
                        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) {
                            Ok(replacement) => {
                                let _ = UnhookWindowsHookEx(hook);
                                hook = replacement;
                            }
                            Err(error) => {
                                tracing::warn!(%error, "could not refresh popup keyboard hook")
                            }
                        }
                        continue;
                    }
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn escape_remains_global_across_hint_selection_and_swallows_repeats() {
        let (tx, rx) = crate::queue::channel(8);
        assert!(STATE.set((tx, RwLock::new(vec![]))).is_ok());
        // Call the hook directly: no system-wide input injection or live hooks.
        let key = |vk, message| {
            let event = KBDLLHOOKSTRUCT {
                vkCode: vk,
                ..Default::default()
            };
            let result = unsafe {
                keyboard(
                    0,
                    WPARAM(message as usize),
                    LPARAM(&event as *const _ as isize),
                )
            };
            assert_eq!(result, LRESULT(1));
        };
        BAR_HINTS.store(1, Relaxed);
        key(0x1b, WM_KEYDOWN);
        assert!(matches!(rx.try_recv().unwrap(), Event::Escape));
        key(0x1b, WM_KEYUP);

        // Select, then Escape, before the UI thread has processed either key.
        key(0x31, WM_KEYDOWN);
        key(0x1b, WM_KEYDOWN);
        assert!(matches!(
            rx.try_recv().unwrap(),
            Event::BarHintKey(1, crate::bar_hints::Input::Select(0))
        ));
        assert!(matches!(rx.try_recv().unwrap(), Event::Escape));
        BAR_HINTS.store(0, Relaxed);
        POPUP_OPEN.store(true, Relaxed);
        key(0x1b, WM_KEYDOWN);
        assert!(
            rx.try_recv().is_err(),
            "held Escape must not repeat into the old window"
        );
        key(0x1b, WM_KEYUP);
        key(0x31, WM_KEYUP);
        key(0x1b, WM_KEYDOWN);
        assert!(matches!(rx.try_recv().unwrap(), Event::Escape));
        key(0x1b, WM_KEYUP);
        POPUP_OPEN.store(false, Relaxed);
    }
}
