//! Opt-in tests: run only on an interactive Windows desktop with Illium running.
#![cfg(windows)]
use std::{sync::mpsc, time::Duration};
use windows::{
    Win32::{
        Foundation::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn ctl(s: &str) -> serde_json::Value {
    let reply = illium::platform::ipc::client(s).expect("running daemon");
    assert!(reply.ok, "{s}: {}", reply.message);
    if s == "status" {
        serde_json::from_str(&reply.message).unwrap()
    } else {
        std::thread::sleep(Duration::from_millis(250));
        serde_json::Value::Null
    }
}
fn status() -> serde_json::Value {
    ctl("status")
}
unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}
fn fixture() -> Vec<isize> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let name = wide("IlliumSmokeFixture");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            lpszClassName: PCWSTR(name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let mut ids = Vec::new();
        for i in 0..4 {
            let title = wide(&format!("Illium smoke {i}"));
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100 + i * 50,
                100,
                400,
                300,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            ids.push(h.0 as isize);
        }
        tx.send(ids).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    rx.recv().unwrap()
}
fn focus(id: isize) {
    unsafe {
        let thread = windows::Win32::System::Threading::GetCurrentThreadId();
        let other = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let _ = windows::Win32::System::Threading::AttachThreadInput(thread, other, true);
        let _ = SetForegroundWindow(HWND(id as *mut _));
        let _ = windows::Win32::System::Threading::AttachThreadInput(thread, other, false);
    }
    std::thread::sleep(Duration::from_millis(300));
    eprintln!(
        "requested focus {id}, foreground {} state {}",
        unsafe { GetForegroundWindow().0 as isize },
        status()["focused"]
    );
}
fn keys(keys: &[u16]) {
    unsafe {
        let mut input = Vec::new();
        for (sequence, up) in [
            (keys.to_vec(), false),
            (keys.iter().rev().copied().collect(), true),
        ] {
            for key in sequence {
                input.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(key),
                            dwFlags: if up {
                                KEYEVENTF_KEYUP
                            } else {
                                KEYBD_EVENT_FLAGS(0)
                            },
                            ..Default::default()
                        },
                    },
                });
            }
        }
        assert_eq!(
            SendInput(&input, std::mem::size_of::<INPUT>() as i32),
            input.len() as u32
        );
    }
    std::thread::sleep(Duration::from_millis(400));
}
struct Cleanup(Vec<isize>, u8);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for id in &self.0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(*id as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        let _ = illium::platform::ipc::client(&format!("workspace {}", self.1));
        let _ = illium::platform::ipc::client("theme set catppuccin-mocha");
    }
}
#[test]
#[ignore = "moves desktop windows; requires running Illium and interactive session"]
fn desktop_smoke() {
    unsafe {
        let h = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let mut title = [0u16; 256];
        let n = GetWindowTextW(h, &mut title);
        eprintln!(
            "foreground pid={pid} title={}",
            String::from_utf16_lossy(&title[..n as usize])
        );
    }
    let initial = status();
    let original = initial["workspace"].as_u64().unwrap() as u8;
    ctl("workspace 9");
    let ids = fixture();
    let _cleanup = Cleanup(ids.clone(), original);
    std::thread::sleep(Duration::from_secs(2));
    let s = status();
    for id in &ids {
        assert!(
            s["clients"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"].as_i64() == Some(*id as i64))
        );
    }
    assert!(s["bar_count"].as_u64().unwrap() > 0);
    let clients = s["clients"].as_array().unwrap();
    let rects = clients
        .iter()
        .filter(|c| ids.contains(&(c["id"].as_i64().unwrap() as isize)))
        .map(|c| c["rect"].clone())
        .collect::<Vec<_>>();
    assert!(rects.windows(2).any(|rs| rs[0] != rs[1]));
    focus(ids[0]);
    keys(&[0x12, 0x10, 0x54]);
    assert!(
        status()["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_i64() == Some(ids[0] as i64))
            .unwrap()["floating"]
            .as_bool()
            .unwrap()
    );
    keys(&[0x12, 0x54]);
    focus(ids[0]);
    keys(&[0x12, 0x46]);
    assert!(
        status()["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_i64() == Some(ids[0] as i64))
            .unwrap()["fullscreen"]
            .as_bool()
            .unwrap()
    );
    keys(&[0x12, 0x46]);
    focus(ids[0]);
    ctl("window move-workspace 8 --follow");
    assert_eq!(status()["workspace"], 8);
    ctl("workspace recent");
    assert_eq!(status()["workspace"], 9);
    ctl("workspace recent");
    assert_eq!(status()["workspace"], 8);
    focus(ids[0]);
    ctl("window move-workspace 9 --follow");
    keys(&[0x12, 0x44]);
    assert_eq!(status()["workspace"], 8);
    keys(&[0x12, 0x53]);
    assert_eq!(status()["workspace"], 9);
    for (key, n) in [(0x31, 1), (0x39, 9)] {
        keys(&[0x12, key]);
        assert_eq!(status()["workspace"], n);
    }
    focus(ids[0]);
    keys(&[0x12, 0x10, 0x38]);
    assert_eq!(status()["workspace"], 8);
    ctl("window move-workspace 9 --follow");
    for direction in ["left", "down", "up", "right"] {
        ctl(&format!("window focus {direction}"));
        ctl(&format!("window move {direction}"));
    }
    use illium::command::Direction::{Down, Left, Right, Up};
    for (key, direction) in [
        (0x48, Left),
        (0x4a, Down),
        (0x4b, Up),
        (0x4c, Right),
        (0x25, Left),
        (0x28, Down),
        (0x26, Up),
        (0x27, Right),
    ] {
        let state = status();
        let current = state["focused"].as_i64().unwrap() as isize;
        let geometry = |s: &serde_json::Value| {
            s["clients"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["workspace"] == s["workspace"] && c["floating"] == false)
                .map(|c| {
                    (
                        c["id"].as_i64().unwrap() as isize,
                        serde_json::from_value::<illium::layout::Rect>(c["rect"].clone()).unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let expected =
            illium::layout::neighbor(&geometry(&state), current, direction).unwrap_or(current);
        keys(&[0x12, key]);
        assert_eq!(
            unsafe { GetForegroundWindow().0 as isize },
            expected,
            "directional shortcut must change actual foreground"
        );
        let state = status();
        let mut expected_order = state["clients"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap() as isize)
            .collect::<Vec<_>>();
        if let Some(other) = illium::layout::neighbor(&geometry(&state), expected, direction) {
            let a = expected_order
                .iter()
                .position(|id| *id == expected)
                .unwrap();
            let b = expected_order.iter().position(|id| *id == other).unwrap();
            expected_order.swap(a, b);
        }
        keys(&[0x12, 0x10, key]);
        let actual = status()["clients"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap() as isize)
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected_order,
            "directional movement shortcut key={key:#x} direction={direction:?} must swap window ordering"
        );
    }
    keys(&[0x12, 0x20]);
    assert_eq!(status()["launcher"], true);
    keys(&[0x1b]);
    assert_eq!(status()["launcher"], false);
    ctl("theme set catppuccin-latte");
    assert_eq!(status()["theme"], "catppuccin-latte");
    ctl("theme set catppuccin-mocha");
    keys(&[0x12, 0x10, 0x52]);
    ctl("config reload");
    focus(ids[0]);
    keys(&[0x12, 0x51]);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!unsafe { IsWindow(Some(HWND(ids[0] as *mut _))).as_bool() });
}

#[test]
#[ignore = "creates and repositions real HWNDs; requires a running daemon"]
fn ipc_desktop_smoke() {
    let original = status()["workspace"].as_u64().unwrap() as u8;
    ctl("workspace 9");
    let ids = fixture();
    let _cleanup = Cleanup(ids.clone(), original);
    std::thread::sleep(Duration::from_secs(1));
    ctl("workspace 8");
    ctl("workspace 9");
    let initial = status();
    let focused = initial["focused"].as_i64().unwrap();
    assert!(ids.contains(&(focused as isize)));
    let client = |s: serde_json::Value| {
        s["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_i64() == Some(focused))
            .unwrap()
            .clone()
    };
    let tiled = client(initial.clone())["rect"].clone();
    ctl("window toggle-float");
    assert_eq!(client(status())["floating"], true);
    let floating = client(status())["rect"].clone();
    assert_ne!(floating, tiled);
    ctl("window toggle-fullscreen");
    assert_eq!(client(status())["fullscreen"], true);
    ctl("window toggle-fullscreen");
    assert_eq!(client(status())["rect"], floating);
    ctl("window set-tiling");
    assert_eq!(client(status())["floating"], false);
    ctl("window move-workspace 8 --follow");
    assert_eq!(status()["workspace"], 8);
    assert_eq!(client(status())["workspace"], 8);
    ctl("workspace recent");
    assert_eq!(status()["workspace"], 9);
    unsafe {
        assert!(!IsWindowVisible(HWND(focused as usize as *mut _)).as_bool());
    }
    ctl("workspace recent");
    assert_eq!(status()["workspace"], 8);
    unsafe {
        assert!(IsWindowVisible(HWND(focused as usize as *mut _)).as_bool());
    }
    ctl("window move-workspace 9 --follow");
    let before = client(status())["rect"].clone();
    ctl("window move right");
    assert_ne!(client(status())["rect"], before);
    ctl("window focus left");
    assert_ne!(status()["focused"].as_i64(), Some(focused));
    ctl("launcher toggle");
    assert_eq!(status()["launcher"], true);
    ctl("launcher toggle");
    assert_eq!(status()["launcher"], false);
    ctl("theme set catppuccin-latte");
    assert_eq!(status()["theme"], "catppuccin-latte");
    ctl("theme set catppuccin-mocha");
    let home = illium::config::Config::home();
    let wm = home.join("wm.toml");
    if let Ok(old) = std::fs::read_to_string(&wm) {
        std::fs::write(&wm, old.replace("gap = 16", "gap = 10")).unwrap();
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(status()["gap"], 10);
        std::fs::write(&wm, "not valid TOML").unwrap();
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(status()["gap"], 10);
        assert!(!illium::platform::ipc::client("config reload").unwrap().ok);
        std::fs::write(&wm, old).unwrap();
        ctl("config reload");
        assert_eq!(status()["gap"], 6);
    }
    let closing = status()["focused"].as_i64().unwrap();
    assert!(ids.contains(&(closing as isize)));
    ctl("window close");
    assert!(!unsafe { IsWindow(Some(HWND(closing as usize as *mut _))).as_bool() });
}

#[test]
#[ignore = "kills a disposable Illium daemon; no existing daemon may be running"]
fn crash_restores_hidden_windows() {
    crash_session(false);
}

#[test]
#[ignore = "stops Explorer and kills a disposable daemon; save work and close Explorer windows"]
fn replacement_crash_restores_explorer() {
    crash_session(true);
}

struct TestDaemon(std::process::Child);
impl Drop for TestDaemon {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn crash_session(replace: bool) {
    let explorer_running = || unsafe {
        let name = wide("Shell_TrayWnd");
        FindWindowW(PCWSTR(name.as_ptr()), None).is_ok()
    };
    if replace {
        assert!(explorer_running(), "start Explorer before this test");
    }
    let exe =
        std::env::var("ILLIUM_TEST_DAEMON").expect("set ILLIUM_TEST_DAEMON to test executable");
    let mut command = std::process::Command::new(exe);
    if replace {
        command.arg("--replace-explorer");
    }
    let mut daemon = TestDaemon(command.spawn().unwrap());
    for _ in 0..100 {
        if illium::platform::ipc::client("status").is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(2));
    if replace {
        assert!(!explorer_running(), "Explorer was not stopped");
    }
    let ids = fixture();
    let _cleanup = Cleanup(ids.clone(), 1);
    std::thread::sleep(Duration::from_secs(1));
    ctl("workspace 2");
    for id in &ids {
        assert!(!unsafe { IsWindowVisible(HWND(*id as *mut _)).as_bool() });
    }
    daemon.0.kill().unwrap();
    daemon.0.wait().unwrap();
    for _ in 0..100 {
        if ids
            .iter()
            .all(|id| unsafe { IsWindowVisible(HWND(*id as *mut _)).as_bool() })
            && (!replace || explorer_running())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("watchdog did not restore hidden fixtures and required Explorer state");
}
