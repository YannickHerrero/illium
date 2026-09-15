//! Opt-in fast path for the bundled terminal alias: no launcher process on
//! Alt+Enter. Other terminals, explicit arguments and external paths retain
//! normal CreateProcess semantics.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
static REQUESTED: Mutex<Option<Instant>> = Mutex::new(None);
fn command() -> Result<String, String> {
    Ok(format!(
        "\"{}\"",
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("winarchy-terminal.exe")
            .display()
    ))
}
pub fn bundled(target: &str) -> bool {
    target.trim().eq_ignore_ascii_case("winarchy-terminal.exe")
        || command().is_ok_and(|c| {
            target.trim().eq_ignore_ascii_case(&c)
                || target.trim().eq_ignore_ascii_case(c.trim_matches('"'))
        })
}
fn request(line: &str, timeout: Duration) -> Result<String, String> {
    let pipe = winarchy_ipc::client::pipe_path(&winarchy_ipc::identity::endpoint_named(
        "winarchy-terminal",
    )?);
    let reply = winarchy_ipc::client::client_at(&pipe, line, timeout)?;
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
                tracing::warn!(%e,"terminal prewarm failed");
            }
        });
    }
}
pub fn wants_focus(exe: &str) -> bool {
    exe.to_ascii_lowercase().ends_with("winarchy-terminal.exe")
        && REQUESTED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take_if(|at| at.elapsed() < Duration::from_secs(3))
            .is_some()
}
pub fn open() {
    *REQUESTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    std::thread::spawn(|| {
        if let Ok(pid) = request("pid", Duration::from_millis(500))
            .and_then(|s| s.parse::<u32>().map_err(|e| e.to_string()))
        {
            unsafe {
                let _ = AllowSetForegroundWindow(pid);
            }
            // Never spawn a fallback after an open with an ambiguous outcome.
            if let Err(e) = request("open", Duration::from_secs(3)) {
                tracing::warn!(%e,"terminal open failed");
            }
        } else if let Err(e) = command().and_then(|c| super::native::spawn(&c)) {
            tracing::warn!(%e,"terminal launch failed");
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
    fn only_bundled_argument_free_launches_use_ipc() {
        assert!(bundled("winarchy-terminal.exe"));
        assert!(bundled(&command().unwrap()));
        assert!(!bundled("wezterm.exe"));
        assert!(!bundled("winarchy-terminal.exe --standalone"));
        assert!(!bundled("C:\\other\\winarchy-terminal.exe"));
    }
}
