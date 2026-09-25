//! Push-to-talk glue for the optional `illium-dictate.exe` resident: the
//! hook reports the `dictate` key going down and up, and the daemon relays
//! `start` and `stop` over the resident's pipe. The daemon never records,
//! transcribes or pastes anything itself, and works without the executable.
use crate::{command::Command, keyboard::Binding};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
const PREFIX: &str = "illium-dictate";
/// What the daemon last asked for; `Command::Dictate` over IPC toggles it.
static RECORDING: AtomicBool = AtomicBool::new(false);
fn executable() -> Result<std::path::PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("illium-dictate.exe"))
}
fn request(line: &str, timeout: Duration) -> Result<String, String> {
    let pipe = illium_ipc::client::pipe_path(&illium_ipc::identity::endpoint_named(PREFIX)?);
    let reply = illium_ipc::client::client_at(&pipe, line, timeout)?;
    if reply.ok {
        Ok(reply.message)
    } else {
        Err(reply.message)
    }
}
fn spawn_resident() {
    match executable() {
        Ok(exe) if exe.is_file() => {
            if let Err(e) = super::native::spawn(&format!("\"{}\" serve", exe.display())) {
                tracing::warn!(%e, "dictation resident not started");
            }
        }
        Ok(exe) => {
            tracing::warn!(path = %exe.display(), "dictate is bound but illium-dictate.exe is missing")
        }
        Err(e) => tracing::warn!(%e, "dictation executable path unavailable"),
    }
}
/// Starts the resident when a `dictate` binding exists, so the first press
/// finds the microphone and the model already open.
pub fn prewarm(bindings: &[Binding]) {
    if !bindings.iter().any(|b| b.command == Command::Dictate) {
        return;
    }
    std::thread::spawn(|| {
        if request("ping", Duration::from_millis(300)).is_err() {
            spawn_resident();
        }
    });
}
/// The bound key went down (`true`) or up (`false`).
pub fn hold(down: bool) {
    RECORDING.store(down, Ordering::Relaxed);
    std::thread::spawn(move || {
        let verb = if down { "start" } else { "stop" };
        if let Err(e) = request(verb, Duration::from_secs(2)) {
            tracing::warn!(%e, verb, "dictation request failed");
            // The press is lost this time; the resident is ready for the next one.
            if down {
                spawn_resident();
            }
        }
    });
}
/// `illiumctl dictate`: start when idle, stop when recording.
pub fn toggle() {
    hold(!RECORDING.load(Ordering::Relaxed));
}
pub fn stop_resident() {
    std::thread::spawn(|| {
        let _ = request("quit", Duration::from_millis(500));
    });
}
