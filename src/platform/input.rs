use super::Event;
use crate::{command::Command, config::Keys};
use std::sync::{OnceLock, RwLock, mpsc::Sender};
use windows::Win32::{
    Foundation::*,
    UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
#[derive(Clone)]
pub struct Binding {
    key: u32,
    modifiers: u8,
    command: Command,
}
static STATE: OnceLock<(Sender<Event>, RwLock<Vec<Binding>>)> = OnceLock::new();
static CONSUMED: std::sync::Mutex<[bool; 256]> = std::sync::Mutex::new([false; 256]);
pub fn parse(keys: &Keys) -> Result<Vec<Binding>, String> {
    keys.keybindings
        .iter()
        .map(|(key, command)| {
            let mut modifiers = 0;
            let mut vk = None;
            for part in key.split('+') {
                match part.to_ascii_lowercase().as_str() {
                    "alt" => modifiers |= 1,
                    "ctrl" => modifiers |= 2,
                    "shift" => modifiers |= 4,
                    "super" => modifiers |= 8,
                    p => {
                        vk = Some(match p {
                            "space" => 32,
                            "enter" => 13,
                            "left" => 37,
                            "up" => 38,
                            "right" => 39,
                            "down" => 40,
                            "escape" => 27,
                            "tab" => 9,
                            s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => {
                                s.to_ascii_uppercase().as_bytes()[0] as u32
                            }
                            _ => return Err(format!("unsupported key: {key}")),
                        });
                    }
                }
            }
            Ok(Binding {
                key: vk.ok_or_else(|| format!("missing key: {key}"))?,
                modifiers,
                command: command.parse()?,
            })
        })
        .collect()
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
            if k.vkCode < 256 {
                let down = w.0 as u32 == WM_KEYDOWN || w.0 as u32 == WM_SYSKEYDOWN;
                let up = w.0 as u32 == WM_KEYUP || w.0 as u32 == WM_SYSKEYUP;
                if up {
                    let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                    if consumed[k.vkCode as usize] {
                        consumed[k.vkCode as usize] = false;
                        return LRESULT(1);
                    }
                }
                if down {
                    let pressed = |v: VIRTUAL_KEY| GetAsyncKeyState(v.0 as i32) < 0;
                    let modifiers = u8::from(pressed(VK_MENU) || k.flags.0 & LLKHF_ALTDOWN.0 != 0)
                        | u8::from(pressed(VK_CONTROL)) * 2
                        | u8::from(pressed(VK_SHIFT)) * 4
                        | u8::from(pressed(VK_LWIN) || pressed(VK_RWIN)) * 8;
                    if let Some((tx, bindings)) = STATE.get() {
                        if let Some(b) = bindings
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .iter()
                            .find(|b| b.key == k.vkCode && b.modifiers == modifiers)
                        {
                            let mut consumed = CONSUMED.lock().unwrap_or_else(|e| e.into_inner());
                            if !consumed[k.vkCode as usize] {
                                let _ = tx.send(Event::Command(b.command.clone(), None));
                            }
                            consumed[k.vkCode as usize] = true;
                            return LRESULT(1);
                        }
                    }
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
    if object == 0 && !h.is_invalid() {
        if let Some((tx, _)) = STATE.get() {
            let _ = tx.send(Event::Window(event, h.0 as isize));
        }
    }
}
pub fn start(tx: Sender<Event>, bindings: Vec<Binding>) -> Result<(), String> {
    let _ = STATE.set((tx, RwLock::new(bindings)));
    let (ready, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0);
        match hook {
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
            }
            Ok(hook) => {
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
                        EVENT_SYSTEM_MOVESIZEEND,
                        EVENT_SYSTEM_MOVESIZEEND,
                        None,
                        Some(window_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    ),
                ];
                let _ = ready.send(Ok(()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
                for h in hooks {
                    let _ = UnhookWinEvent(h);
                }
            }
        }
    });
    rx.recv().map_err(|e| e.to_string())?
}
