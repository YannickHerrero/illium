//! Physical Escape fallback for passive popups. Raw input is a notification,
//! not a replacement for the consuming low-level keyboard hook.
use super::*;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RAWKEYBOARD, RID_INPUT,
    RIDEV_INPUTSINK, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};

#[derive(Default)]
struct EscapeKey {
    held: bool,
}
impl EscapeKey {
    fn update(&mut self, key: RAWKEYBOARD, active: bool, hook_consumed: bool) -> bool {
        if key.VKey != VK_ESCAPE.0 {
            return false;
        }
        match key.Message {
            WM_KEYUP | WM_SYSKEYUP => {
                self.held = false;
                false
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let first = !self.held;
                self.held = true;
                first && active && !hook_consumed
            }
            _ => false,
        }
    }
}
static ESCAPE: std::sync::Mutex<EscapeKey> = std::sync::Mutex::new(EscapeKey { held: false });

pub(super) fn register(window: HWND) -> windows::core::Result<()> {
    // Do not use NOLEGACY: regular Slint/client keyboard input must stay intact.
    // Winit has initialized its device registration before input::start runs;
    // the shell does not use its DeviceEvent stream. Only keyboard raw input is
    // routed here, leaving winit's mouse registration untouched.
    unsafe {
        RegisterRawInputDevices(
            &[RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: window,
            }],
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )
    }
}

pub(super) fn received(handle: LPARAM) {
    let mut input = RAWINPUT::default();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    let read = unsafe {
        GetRawInputData(
            HRAWINPUT(handle.0 as *mut _),
            RID_INPUT,
            Some((&mut input as *mut RAWINPUT).cast()),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX
        || read
            < (std::mem::size_of::<RAWINPUTHEADER>() + std::mem::size_of::<RAWKEYBOARD>()) as u32
        || input.header.dwType != RIM_TYPEKEYBOARD.0
    {
        return;
    }
    let key = unsafe { input.data.keyboard };
    if key.VKey != VK_ESCAPE.0 {
        return;
    }
    let Some((tx, _)) = STATE.get() else { return };
    use std::sync::atomic::Ordering::Relaxed;
    let popup = POPUP_OPEN.load(Relaxed);
    let hints = BAR_HINTS.load(Relaxed);
    let capture = CAPTURE.load(Relaxed);
    let consumed =
        CONSUMED.lock().unwrap_or_else(|e| e.into_inner())[VK_ESCAPE.0 as usize] != Consumed::No;
    if TRACE_ESCAPE.load(Relaxed) {
        let _ = tx.send(Event::EscapeTrace(EscapeTrace {
            source: "raw",
            down: key.Message == WM_KEYDOWN || key.Message == WM_SYSKEYDOWN,
            popup,
            hints,
            capture,
            consumed,
            flags: key.Flags as u32,
            extra: key.ExtraInformation as usize,
        }));
    }
    if ESCAPE.lock().unwrap_or_else(|e| e.into_inner()).update(
        key,
        (popup || hints != 0) && !capture,
        consumed,
    ) {
        let _ = tx.send(Event::Escape);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(message: u32) -> RAWKEYBOARD {
        RAWKEYBOARD {
            VKey: VK_ESCAPE.0,
            Message: message,
            ..Default::default()
        }
    }
    #[test]
    fn physical_escape_closes_once_without_focus_or_hook_delivery() {
        let mut escape = EscapeKey::default();
        assert!(escape.update(key(WM_KEYDOWN), true, false));
        assert!(!escape.update(key(WM_KEYDOWN), true, false));
        assert!(!escape.update(key(WM_KEYUP), false, false));
        assert!(escape.update(key(WM_SYSKEYDOWN), true, false));
        assert!(!escape.update(key(WM_SYSKEYUP), true, false));
    }
    #[test]
    fn raw_notifications_do_not_steal_regular_keys_or_repeat_consumed_escape() {
        let mut escape = EscapeKey::default();
        assert!(!escape.update(key(WM_KEYDOWN), false, false));
        assert!(!escape.update(key(WM_KEYDOWN), true, false)); // held before popup
        escape.update(key(WM_KEYUP), true, false);
        assert!(!escape.update(key(WM_KEYDOWN), true, true)); // hook already handled it
        assert!(!escape.update(key(WM_KEYDOWN), true, false));
        escape.update(key(WM_KEYUP), true, false);
        let other = RAWKEYBOARD {
            VKey: 65,
            Message: WM_KEYDOWN,
            ..Default::default()
        };
        assert!(!escape.update(other, true, false));
        assert!(escape.update(key(WM_KEYDOWN), true, false));
    }
}
