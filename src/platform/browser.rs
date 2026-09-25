//! Same launch pattern as the terminal: prepare one hidden browser at shell
//! startup, then open it through an owner-only pipe without a launcher process.
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
fn command() -> Result<String, String> {
    Ok(format!(
        "\"{}\"",
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("illium-browser.exe")
            .display()
    ))
}
pub fn bundled(target: &str) -> bool {
    target.trim().eq_ignore_ascii_case("illium-browser.exe")
        || command().is_ok_and(|c| {
            target.trim().eq_ignore_ascii_case(&c)
                || target.trim().eq_ignore_ascii_case(c.trim_matches('"'))
        })
}
fn request(command: &str, timeout: Duration) -> Result<String, String> {
    let pipe =
        illium_ipc::client::pipe_path(&illium_ipc::identity::endpoint_named("illium-browser")?);
    let reply = illium_ipc::client::client_at(&pipe, command, timeout)?;
    if reply.ok {
        Ok(reply.message)
    } else {
        Err(reply.message)
    }
}
pub fn prewarm(target: Option<&String>) {
    if target.is_some_and(|s| bundled(s)) {
        std::thread::spawn(|| {
            if let Err(e) = command().and_then(|c| super::native::spawn(&format!("{c} --serve"))) {
                tracing::warn!(%e, "browser prewarm failed");
            }
        });
    }
}
pub fn open() {
    std::thread::spawn(|| {
        let started = std::time::Instant::now();
        if let Ok(pid) = request("pid", Duration::from_millis(500))
            .and_then(|s| s.parse::<u32>().map_err(|e| e.to_string()))
        {
            unsafe {
                let _ = AllowSetForegroundWindow(pid);
            }
            // No fallback after an open: an ambiguous result could duplicate a window.
            if let Err(e) = request("open \"\"", Duration::from_secs(3)) {
                tracing::warn!(%e, "browser open failed");
            } else {
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "prepared browser opened"
                );
            }
        } else if let Err(e) = command().and_then(|c| super::native::spawn(&c)) {
            tracing::warn!(%e, "browser launch failed");
        }
    });
}
pub fn stop_idle() {
    std::thread::spawn(|| {
        let _ = request("quit", Duration::from_millis(500));
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bundled_argument_free_aliases_use_resident() {
        assert!(bundled("illium-browser.exe"));
        assert!(bundled(&command().unwrap()));
        assert!(!bundled("illium-browser.exe --standalone"));
        assert!(!bundled("illium-browser.exe https://example.com"));
        assert!(!bundled("C:\\other\\illium-browser.exe"));
        assert!(!bundled("msedge.exe"));
    }
}
