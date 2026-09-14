//! Opt-in live benchmark: profile_theme <theme> ... (restores selection files).
#[cfg(windows)]
fn main() -> Result<(), String> {
    use std::{fs, time::Instant};
    use winarchy_ipc::client::client;
    fn run(command: &str) -> Result<(), String> {
        let start = Instant::now();
        let reply = client(command)?;
        if !reply.ok {
            return Err(reply.message);
        }
        println!("{command}: {} ms", start.elapsed().as_millis());
        Ok(())
    }
    let home = winarchy_theme::config_home();
    let global = fs::read(home.join("winarchy.toml")).map_err(|e| e.to_string())?;
    let state = fs::read(home.join("wallpapers.json")).ok();
    let result = (|| {
        for name in std::env::args().skip(1) {
            if !winarchy_theme::valid_name(&name) {
                return Err("invalid theme name".into());
            }
            run(&format!("theme set {name}"))?;
            run("wallpaper next")?;
            run("wallpaper next")?;
            run("config reload")?;
        }
        Ok(())
    })();
    // Always restore before propagating a benchmark failure.
    fs::write(home.join("winarchy.toml"), global).map_err(|e| e.to_string())?;
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
