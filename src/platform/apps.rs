//! Companion applications: shown through the resident `illium-apps serve`
//! process when it answers, started as their own process otherwise.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
const PREFIX: &str = "illium-apps";
/// When a show request last went out: the resident process is not the
/// foreground process, so Windows refuses it the foreground; the daemon
/// focuses the window itself when it appears shortly after a request.
static REQUESTED: Mutex<Option<Instant>> = Mutex::new(None);
/// Whether a window of `exe` appearing now was asked for by the user.
pub fn wants_focus(exe: &str) -> bool {
    exe.to_lowercase().ends_with("illium-apps.exe")
        && REQUESTED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take_if(|at| at.elapsed() < Duration::from_secs(3))
            .is_some()
}
/// Command line running application `name` of the `illium-apps.exe` next to the daemon.
pub fn command(name: &str) -> Result<String, String> {
    let tool = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("illium-apps.exe");
    Ok(format!("\"{}\" {name}", tool.display()))
}
fn pipe() -> Result<String, String> {
    Ok(illium_ipc::client::pipe_path(
        &illium_ipc::identity::endpoint_named(PREFIX)?,
    ))
}
fn request(line: &str, timeout: Duration) -> Result<(), String> {
    let reply = if line.starts_with("show ") {
        illium_ipc::client::client_at_foreground(&pipe()?, line, timeout)?
    } else {
        illium_ipc::client::client_at(&pipe()?, line, timeout)?
    };
    if reply.ok { Ok(()) } else { Err(reply.message) }
}
/// Starts the resident process; a second instance exits on its own.
pub fn start_resident() {
    if let Err(e) = command("serve").and_then(|c| super::native::spawn(&c)) {
        tracing::warn!(%e, "resident applications not started");
    }
}
pub fn stop_resident() {
    let _ = request("quit", Duration::from_millis(500));
}
/// Shows `name` without blocking the caller: the pipe round trip, or the
/// fallback spawn, runs on its own thread.
pub fn open(name: String) {
    std::thread::spawn(move || {
        if request(&format!("show {name}"), Duration::from_secs(2)).is_ok() {
            return;
        }
        start_resident();
        if let Err(e) = command(&name).and_then(|c| super::native::spawn(&c)) {
            tracing::warn!(app = %name, %e, "application not started");
        }
    });
}
