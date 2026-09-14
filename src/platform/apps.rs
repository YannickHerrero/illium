//! Companion applications: shown through the resident `winarchy-apps serve`
//! process when it answers, started as their own process otherwise.
use std::time::Duration;
const PREFIX: &str = "winarchy-apps";
/// Command line running application `name` of the `winarchy-apps.exe` next to the daemon.
pub fn command(name: &str) -> Result<String, String> {
    let tool = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("winarchy-apps.exe");
    Ok(format!("\"{}\" {name}", tool.display()))
}
fn pipe() -> Result<String, String> {
    Ok(winarchy_ipc::client::pipe_path(
        &winarchy_ipc::identity::endpoint_named(PREFIX)?,
    ))
}
fn request(line: &str, timeout: Duration) -> Result<(), String> {
    let reply = winarchy_ipc::client::client_at(&pipe()?, line, timeout)?;
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
        if name != "shot" && request(&format!("show {name}"), Duration::from_secs(2)).is_ok() {
            return;
        }
        if name != "shot" {
            start_resident();
        }
        if let Err(e) = command(&name).and_then(|c| super::native::spawn(&c)) {
            tracing::warn!(app = %name, %e, "application not started");
        }
    });
}
