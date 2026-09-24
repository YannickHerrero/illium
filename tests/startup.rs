//! Starts a daemon after the fixture windows already exist.
#![cfg(windows)]
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::PCWSTR,
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}
fn fixtures() -> Vec<isize> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let name = wide("WinarchyPreexistingFixture");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            lpszClassName: PCWSTR(name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let mut ids = vec![];
        for i in 0..4 {
            let title = wide(&format!("Winarchy existing {i}"));
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100 + i * 30,
                100,
                450,
                350,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            ids.push(h.0 as isize);
        }
        let _ = ShowWindow(HWND(ids[3] as *mut _), SW_MINIMIZE);
        tx.send(ids).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    rx.recv().unwrap()
}
struct Session {
    daemon: std::process::Child,
    ids: Vec<isize>,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = winarchy::platform::ipc::client("quit");
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until && self.daemon.try_wait().ok().flatten().is_none() {
            std::thread::sleep(Duration::from_millis(50));
        }
        if self.daemon.try_wait().ok().flatten().is_none() {
            let _ = self.daemon.kill();
        }
        let _ = self.daemon.wait();
        for id in &self.ids {
            unsafe {
                let _ = PostMessageW(Some(HWND(*id as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}
fn state() -> Option<serde_json::Value> {
    let reply = winarchy::platform::ipc::client("status").ok()?;
    if !reply.ok {
        return None;
    }
    serde_json::from_str(&reply.message).ok()
}
fn wait_for(test: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        if let Some(s) = state()
            && test(&s)
        {
            return s;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("preexisting windows were not enrolled/restored before the deadline");
}
fn contains(s: &serde_json::Value, id: isize) -> bool {
    s["clients"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"].as_i64() == Some(id as i64))
}
#[test]
#[ignore = "rearranges existing desktop windows; set WINARCHY_TEST_DAEMON and use a disposable config, no running daemon"]
fn enrolls_preexisting_windows_and_tracks_restoration() {
    assert!(state().is_none(), "stop Winarchy before the startup test");
    let exe = std::env::var("WINARCHY_TEST_DAEMON").expect("set test daemon path");
    let ids = fixtures();
    let _session = Session {
        daemon: std::process::Command::new(exe).spawn().unwrap(),
        ids: ids.clone(),
    };
    let s = wait_for(|s| ids[..3].iter().all(|id| contains(s, *id)));
    assert!(!contains(&s, ids[3]));
    assert!(unsafe { IsIconic(HWND(ids[3] as *mut _)).as_bool() });
    let rects = s["clients"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| ids[..3].contains(&(c["id"].as_i64().unwrap() as isize)))
        .map(|c| {
            assert_eq!(c["workspace"], s["workspace"]);
            assert_eq!(c["floating"], false);
            serde_json::from_value::<winarchy::layout::Rect>(c["rect"].clone()).unwrap()
        })
        .collect::<Vec<_>>();
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            assert!(
                a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
                "preexisting windows should be tiled rather than overlapping"
            );
        }
    }
    unsafe {
        let _ = ShowWindow(HWND(ids[3] as *mut _), SW_RESTORE);
    }
    wait_for(|s| contains(s, ids[3]));
    unsafe {
        let _ = ShowWindow(HWND(ids[0] as *mut _), SW_MINIMIZE);
    }
    std::thread::sleep(Duration::from_millis(300));
    let reply = winarchy::platform::ipc::client("config reload").unwrap();
    assert!(reply.ok);
    assert!(
        unsafe { IsIconic(HWND(ids[0] as *mut _)).as_bool() },
        "relayout must not unminimize clients"
    );
    unsafe {
        let _ = ShowWindow(HWND(ids[0] as *mut _), SW_RESTORE);
    }
    wait_for(|s| ids.iter().all(|id| contains(s, *id)));
}
#[test]
#[ignore = "rearranges existing desktop windows; set WINARCHY_TEST_DAEMON and use a disposable config, no running daemon"]
fn restores_remembered_placement_and_drops_stale_entries() {
    assert!(state().is_none(), "stop Winarchy before the startup test");
    let exe = std::env::var("WINARCHY_TEST_DAEMON").expect("set test daemon path");
    let ids = fixtures();
    let placement = |id: isize, pid: u32, workspace: u8| winarchy::state::Placement {
        id,
        pid,
        exe: std::env::current_exe().unwrap().display().to_string(),
        space: 0,
        workspace,
        floating: false,
        fullscreen: false,
        restore: winarchy::layout::Rect::default(),
    };
    let saved = winarchy::state::State {
        active: 3,
        recent: 1,
        monitors: [0; 9],
        clients: vec![
            placement(ids[1], std::process::id(), 3),
            // Same handle number, foreign process: a reused HWND must be ignored.
            placement(ids[2], std::process::id() + 1, 5),
            placement(0x7fff_0001, std::process::id(), 7),
        ],
        ..Default::default()
    };
    let path = winarchy::config::Config::home().join("state.json");
    winarchy::state::State::save(&saved.to_json(), &path).unwrap();
    let _session = Session {
        daemon: std::process::Command::new(exe).spawn().unwrap(),
        ids: ids.clone(),
    };
    let s = wait_for(|s| ids[..3].iter().all(|id| contains(s, *id)));
    assert_eq!(s["workspace"], 3);
    let workspace = |id: isize| {
        s["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_i64() == Some(id as i64))
            .map(|c| c["workspace"].as_u64().unwrap())
            .unwrap()
    };
    assert_eq!(workspace(ids[1]), 3, "remembered placement restored");
    assert_eq!(workspace(ids[0]), 1, "unknown windows land on workspace 1");
    assert_eq!(workspace(ids[2]), 1, "foreign-process entry ignored");
    // The daemon rewrites the file on its one-second maintenance tick.
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until
        && winarchy::state::State::load(&path)
            .is_some_and(|s| s.clients.iter().any(|c| c.id == 0x7fff_0001))
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    let reloaded = winarchy::state::State::load(&path).unwrap();
    assert!(
        reloaded.clients.iter().all(|c| c.id != 0x7fff_0001),
        "stale entries are dropped from the saved state"
    );
}
