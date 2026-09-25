//! Opt-in launcher/terminal tests on an unlocked desktop, alongside Explorer.
#![cfg(windows)]
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::Threading::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::PWSTR,
};
fn status() -> serde_json::Value {
    let r = illium::platform::ipc::client("status").unwrap();
    assert!(r.ok);
    serde_json::from_str(&r.message).unwrap()
}
fn send(events: &[(u16, u16, KEYBD_EVENT_FLAGS)]) {
    let inputs = events
        .iter()
        .map(|(key, scan, flags)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(*key),
                    wScan: *scan,
                    dwFlags: *flags,
                    ..Default::default()
                },
            },
        })
        .collect::<Vec<_>>();
    assert_eq!(
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) },
        inputs.len() as u32
    );
    std::thread::sleep(Duration::from_millis(250));
}
fn chord(key: u16) {
    send(&[
        (0x12, 0, KEYBD_EVENT_FLAGS(0)),
        (key, 0, KEYBD_EVENT_FLAGS(0)),
        (key, 0, KEYEVENTF_KEYUP),
        (0x12, 0, KEYEVENTF_KEYUP),
    ]);
}
fn key(key: u16) {
    send(&[(key, 0, KEYBD_EVENT_FLAGS(0)), (key, 0, KEYEVENTF_KEYUP)]);
}
fn type_text(text: &str) {
    let events = text
        .encode_utf16()
        .flat_map(|c| {
            [
                (0, c, KEYEVENTF_UNICODE),
                (0, c, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        })
        .collect::<Vec<_>>();
    send(&events);
}
fn is_wezterm(id: isize) -> bool {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(HWND(id as *mut _), Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut data = [0u16; 2048];
        let mut len = data.len() as u32;
        let result =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(data.as_mut_ptr()), &mut len)
                .is_ok()
                && String::from_utf16_lossy(&data[..len as usize])
                    .to_lowercase()
                    .ends_with("\\wezterm-gui.exe");
        let _ = CloseHandle(h);
        result
    }
}
fn before() -> HashSet<isize> {
    status()["clients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap() as isize)
        .collect()
}
fn await_terminal(old: HashSet<isize>) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for c in status()["clients"].as_array().unwrap() {
            let id = c["id"].as_i64().unwrap() as isize;
            if !old.contains(&id) && is_wezterm(id) {
                unsafe {
                    let _ = PostMessageW(Some(HWND(id as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("no newly managed WezTerm GUI window appeared");
}
#[test]
#[ignore = "requires running Illium, WezTerm and an unlocked desktop"]
fn alt_enter_launches_wezterm() {
    let old = before();
    chord(0x0d);
    await_terminal(old);
}
#[test]
#[ignore = "types in the launcher and starts WezTerm on an unlocked desktop"]
fn launcher_search_navigation_and_enter() {
    launch_from_search("terminal");
}
#[test]
#[ignore = "requires the installed WezTerm Start Menu shortcut and an unlocked desktop"]
fn launcher_launches_start_menu_shortcut() {
    launch_from_search("wezterm");
}
fn launch_from_search(query: &str) {
    let old = before();
    if status()["launcher"] == true {
        chord(0x20);
    }
    chord(0x20);
    assert_eq!(status()["launcher"], true);
    type_text(query);
    key(0x28);
    key(0x26);
    key(0x0d);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(status()["launcher"], false);
    await_terminal(old);
}
