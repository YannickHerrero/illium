//! Opt-in test with real Windows hooks. It swallows only the Escape events it
//! injects; no focus changes, popup activation or input to the foreground app.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

static INTERCEPTED: AtomicUsize = AtomicUsize::new(0);
const TEST_INPUT: usize = 0x5741_4553;
unsafe extern "system" fn client_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let key = &*(l.0 as *const KBDLLHOOKSTRUCT);
            if key.vkCode == VK_ESCAPE.0 as u32 && key.dwExtraInfo == TEST_INPUT {
                INTERCEPTED.fetch_add(1, Relaxed);
                return LRESULT(1);
            }
        }
        CallNextHookEx(None, code, w, l)
    }
}
fn pump() {
    let until = Instant::now() + Duration::from_millis(150);
    unsafe {
        let mut message = MSG::default();
        while Instant::now() < until {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
fn escape(up: bool) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_ESCAPE,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                dwExtraInfo: TEST_INPUT,
                ..Default::default()
            },
        },
    };
    assert_eq!(
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) },
        1
    );
    pump();
}

#[test]
#[ignore = "real desktop hooks; run alone with --ignored --test-threads=1"]
fn passive_popup_escape_precedes_a_newer_client_hook_without_focus() {
    let (tx, rx) = crate::queue::channel(1024);
    start(tx, vec![]).unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    // Model a foreground app that intercepts Escape after Illium has started.
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(client_hook), None, 0) }.unwrap();
    struct Hook(HHOOK);
    impl Drop for Hook {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }
    let _hook = Hook(hook);
    escape(false);
    escape(true);
    assert_eq!(INTERCEPTED.load(Relaxed), 2);
    assert!(!rx.try_iter().any(|event| matches!(event, Event::Escape)));

    // This is the same publication used after a mouse-opened, nonactivating
    // applet. No click inside it and no SetForegroundWindow call are necessary.
    set_popup_open(true);
    pump();
    escape(false);
    assert!(rx.try_iter().any(|event| matches!(event, Event::Escape)));
    assert_eq!(INTERCEPTED.load(Relaxed), 2);
    set_popup_open(false);
    escape(false); // held key repeat after the popup has closed
    escape(true);
    assert!(!rx.try_iter().any(|event| matches!(event, Event::Escape)));
    assert_eq!(
        INTERCEPTED.load(Relaxed),
        2,
        "repeat/release leaked into the client"
    );
    assert_eq!(
        unsafe { GetForegroundWindow() },
        foreground,
        "popup capture stole focus"
    );

    escape(false);
    escape(true);
    assert_eq!(
        INTERCEPTED.load(Relaxed),
        4,
        "closed popup still owns Escape"
    );
}
