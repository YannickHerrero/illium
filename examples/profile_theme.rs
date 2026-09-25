//! Opt-in live benchmark: profile_theme <theme> ... (restores selection files).
#[cfg(windows)]
fn main() -> Result<(), String> {
    use illium_ipc::client::client;
    use std::{fs, time::Instant};
    fn run(command: &str) -> Result<(), String> {
        let start = Instant::now();
        let reply = client(command)?;
        if !reply.ok {
            return Err(reply.message);
        }
        let acknowledged = start.elapsed().as_millis();
        let mut max_status_ms = 0;
        loop {
            let query = Instant::now();
            let reply = client("status")?;
            max_status_ms = max_status_ms.max(query.elapsed().as_millis());
            if !reply.ok {
                return Err(reply.message);
            }
            let status: serde_json::Value =
                serde_json::from_str(&reply.message).map_err(|e| e.to_string())?;
            if status["wallpaper_pending"].is_null() {
                break;
            }
            if start.elapsed().as_secs() >= 15 {
                return Err("wallpaper did not complete".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        println!(
            "{command}: ack={acknowledged} ms, ready={} ms, max_status={max_status_ms} ms",
            start.elapsed().as_millis()
        );
        Ok(())
    }
    let home = illium_theme::config_home();
    let global = fs::read(home.join("illium.toml")).map_err(|e| e.to_string())?;
    let state = fs::read(home.join("wallpapers.json")).ok();
    let result = (|| {
        for name in std::env::args().skip(1) {
            if !illium_theme::valid_name(&name) {
                return Err("invalid theme name".into());
            }
            run(&format!("theme set {name}"))?;
            // Let the next image finish preloading, then measure the warm cycle.
            std::thread::sleep(std::time::Duration::from_secs(2));
            run("wallpaper next")?;
            run("wallpaper next")?;
            run("config reload")?;
        }
        Ok(())
    })();
    // Always restore before propagating a benchmark failure.
    fs::write(home.join("illium.toml"), global).map_err(|e| e.to_string())?;
    if let Some(state) = state {
        fs::write(home.join("wallpapers.json"), state).map_err(|e| e.to_string())?;
    } else {
        let _ = fs::remove_file(home.join("wallpapers.json"));
    }
    let restore = run("config reload");
    result.and(restore)
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This opt-in IPC benchmark requires a running Windows daemon.");
}
