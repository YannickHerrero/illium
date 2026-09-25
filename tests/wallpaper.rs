//! Opt-in IPC test. Temporarily changes the desktop theme; restores original files.
#![cfg(windows)]
use illium_ipc::client::client;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
const IMAGE: &[u8] = include_bytes!("fixtures/wallpaper.png");
fn command(s: &str) {
    let reply = client(s).expect("running Illium");
    assert!(reply.ok, "{s}: {}", reply.message);
}
fn status() -> serde_json::Value {
    let reply = client("status").expect("running Illium");
    assert!(reply.ok, "{}", reply.message);
    serde_json::from_str(&reply.message).unwrap()
}
fn wait_wallpaper(name: Option<&str>) {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let current = status();
        if current["wallpaper"].as_str() == name && current["wallpaper_pending"].is_null() {
            return;
        }
        assert!(Instant::now() < deadline, "expected wallpaper {name:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct Restore {
    home: PathBuf,
    name: String,
    global: Vec<u8>,
    selections: Option<Vec<u8>>,
}
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::write(self.home.join("illium.toml"), &self.global);
        match &self.selections {
            Some(bytes) => {
                let _ = std::fs::write(self.home.join("wallpapers.json"), bytes);
            }
            None => {
                let _ = std::fs::remove_file(self.home.join("wallpapers.json"));
            }
        }
        let _ = client("config reload");
        let _ = std::fs::remove_file(self.home.join("themes").join(format!("{}.toml", self.name)));
        let _ = std::fs::remove_dir_all(self.home.join("themes").join(&self.name));
    }
}
#[test]
#[ignore = "temporarily changes themes; requires a running upgraded daemon with the same config home"]
fn wallpapers_follow_selection_and_directory_changes() {
    let initial = status();
    let home = illium_theme::config_home();
    let name = format!("wallpaper-smoke-{}", std::process::id());
    let dir = home.join("themes").join(&name);
    assert!(!dir.exists());
    assert!(!home.join("themes").join(format!("{name}.toml")).exists());
    let _restore = Restore {
        global: std::fs::read(home.join("illium.toml")).unwrap(),
        selections: std::fs::read(home.join("wallpapers.json")).ok(),
        home: home.clone(),
        name: name.clone(),
    };
    std::fs::create_dir_all(dir.join("wallpapers")).unwrap();
    std::fs::write(
        home.join("themes").join(format!("{name}.toml")),
        include_str!("../config/themes/catppuccin-mocha.toml"),
    )
    .unwrap();
    command(&format!("theme set {name}"));
    wait_wallpaper(None);
    let a = dir.join("wallpapers/A painting.png");
    let b = dir.join("wallpapers/B painting.png");
    std::fs::write(&b, IMAGE).unwrap();
    wait_wallpaper(Some("B painting.png"));
    std::fs::write(&a, IMAGE).unwrap();
    wait_wallpaper(Some("A painting.png"));
    command("wallpaper next");
    wait_wallpaper(Some("B painting.png"));
    command("wallpaper next");
    wait_wallpaper(Some("A painting.png"));
    command("wallpaper clear");
    wait_wallpaper(None);
    command(&format!("theme set {}", initial["theme"].as_str().unwrap()));
    command(&format!("theme set {name}"));
    wait_wallpaper(None);
    command("wallpaper set \"A painting.png\"");
    command("config reload");
    wait_wallpaper(Some("A painting.png"));
    assert!(!client("wallpaper set missing.png").unwrap().ok);
    wait_wallpaper(Some("A painting.png"));
    std::fs::write(&a, "corrupt image").unwrap();
    wait_wallpaper(Some("B painting.png"));
    std::fs::remove_file(&b).unwrap();
    wait_wallpaper(None);
    std::fs::write(&a, IMAGE).unwrap();
    wait_wallpaper(Some("A painting.png"));
    // Rapid nexts use the pending target, not the old displayed image.
    std::fs::write(&b, IMAGE).unwrap();
    command("wallpaper next");
    command("wallpaper next");
    wait_wallpaper(Some("A painting.png"));
    // Clear and theme changes cancel in-flight work, including prefetched results.
    command("wallpaper next");
    command("wallpaper clear");
    wait_wallpaper(None);
    std::thread::sleep(Duration::from_millis(500));
    wait_wallpaper(None);
    command("wallpaper set A painting.png");
    wait_wallpaper(Some("A painting.png"));
    // An explicit corrupt image is rejected asynchronously without losing the last choice.
    let bad = dir.join("wallpapers/broken.png");
    std::fs::write(&bad, "not png").unwrap();
    command("wallpaper set broken.png");
    wait_wallpaper(Some("A painting.png"));
    assert!(status()["wallpaper_error"].is_string());
    // Invalid assets did not force a change of palette/theme.
    assert_eq!(status()["theme"], name);
}
