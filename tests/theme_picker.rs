//! Opt-in interactive test. Uses a disposable config; temporarily changes theme.
#![cfg(windows)]
use illium_ipc::client::client;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};

fn command(line: &str) {
    let r = client(line).unwrap();
    assert!(r.ok, "{line}: {}", r.message);
}
fn status() -> serde_json::Value {
    serde_json::from_str(&client("status").unwrap().message).unwrap()
}
fn wait(test: impl Fn(&serde_json::Value) -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let s = status();
        if test(&s) {
            return;
        }
        assert!(Instant::now() < until, "status: {s}");
        std::thread::sleep(Duration::from_millis(30));
    }
}
fn send(vk: VIRTUAL_KEY, scan: u16, unicode: bool) {
    let flags = if unicode {
        KEYEVENTF_UNICODE
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let event = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    assert_eq!(
        unsafe {
            SendInput(
                &[event(flags), event(flags | KEYEVENTF_KEYUP)],
                std::mem::size_of::<INPUT>() as i32,
            )
        },
        2
    );
}
fn picker_hwnd() -> HWND {
    unsafe { FindWindowW(None, w!("Illium Theme Picker")) }.unwrap()
}
struct Restore {
    home: PathBuf,
    name: String,
    global: Vec<u8>,
    wallpapers: Option<Vec<u8>>,
}
impl Drop for Restore {
    fn drop(&mut self) {
        if client("status")
            .ok()
            .and_then(|reply| serde_json::from_str::<serde_json::Value>(&reply.message).ok())
            .is_some_and(|s| s["theme_picker"] == true)
        {
            let _ = client("launcher toggle");
            let _ = client("launcher toggle");
        }
        let _ = fs::write(self.home.join("illium.toml"), &self.global);
        if let Some(bytes) = &self.wallpapers {
            let _ = fs::write(self.home.join("wallpapers.json"), bytes);
        } else {
            let _ = fs::remove_file(self.home.join("wallpapers.json"));
        }
        let _ = client("config reload");
        let _ = fs::remove_file(self.home.join(format!("themes/{}.toml", self.name)));
        let _ = fs::remove_dir_all(self.home.join("themes").join(&self.name));
    }
}
#[test]
#[ignore = "requires an unlocked Windows desktop and running upgraded daemon with a disposable ILLIUM_CONFIG_HOME; changes theme and keyboard focus"]
fn picker_focus_filter_cancel_confirm_and_asset_refresh() {
    let home = illium_theme::config_home();
    let initial = status();
    assert_eq!(initial["theme_picker"], false, "close shell menus first");
    let name = format!("picker-smoke-{}", std::process::id());
    assert!(!home.join("themes").join(&name).exists());
    assert!(!home.join(format!("themes/{name}.toml")).exists());
    let restore = Restore {
        global: fs::read(home.join("illium.toml")).unwrap(),
        wallpapers: fs::read(home.join("wallpapers.json")).ok(),
        home: home.clone(),
        name: name.clone(),
    };
    fs::create_dir(home.join("themes").join(&name)).unwrap();
    let preview = home.join(format!("themes/{name}/preview.png"));
    fs::write(&preview, include_bytes!("fixtures/wallpaper.png")).unwrap();
    fs::write(
        home.join(format!("themes/{name}.toml")),
        include_str!("../config/themes/catppuccin-mocha.toml"),
    )
    .unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    command("theme picker");
    wait(|s| s["theme_picker"] == true && s["theme_picker_loading"] == false);
    assert_eq!(unsafe { GetForegroundWindow() }, picker_hwnd());
    for c in name.encode_utf16() {
        send(VIRTUAL_KEY(0), c, true);
    }
    wait(|s| s["theme_picker_filter"] == name && s["theme_picker_selected"] == name);
    assert_eq!(fs::read(home.join("illium.toml")).unwrap(), restore.global);
    send(VK_ESCAPE, 0, false);
    wait(|s| s["theme_picker"] == true && s["theme_picker_filter"] == "");
    send(VK_ESCAPE, 0, false);
    wait(|s| s["theme_picker"] == false);
    assert_eq!(status()["theme"], initial["theme"]);
    if unsafe { IsWindowVisible(foreground) }.as_bool() {
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
    command("theme picker");
    wait(|s| s["theme_picker"] == true && s["theme_picker_loading"] == false);
    for c in name.encode_utf16() {
        send(VIRTUAL_KEY(0), c, true);
    }
    wait(|s| s["theme_picker_selected"] == name && s["theme_picker_loading"] == false);
    fs::remove_file(&preview).unwrap();
    wait(|s| s["theme_picker_selected"].is_null() && s["theme_picker_loading"] == false);
    fs::write(&preview, include_bytes!("fixtures/wallpaper.png")).unwrap();
    wait(|s| s["theme_picker_selected"] == name && s["theme_picker_loading"] == false);
    send(VK_RETURN, 0, false);
    wait(|s| s["theme_picker"] == false && s["theme"] == name);
}
