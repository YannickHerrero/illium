use crate::{command::Reply, pipe_io::PipeIo, wide};
use std::{
    io::{BufReader, Read, Write},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, Storage::FileSystem::*, System::Pipes::*},
    core::PCWSTR,
};
pub const MAX_REPLY: usize = 65535;
const ACK: u8 = 6;
pub fn pipe_name() -> Result<String, String> {
    Ok(pipe_path(&crate::identity::endpoint()?))
}
pub fn pipe_path(endpoint: &str) -> String {
    format!(r"\\.\pipe\{endpoint}")
}
/// Sends `command` to the daemon.
pub fn client(command: &str) -> Result<Reply, String> {
    client_at(&pipe_name()?, command, Duration::from_secs(12))
}
/// Sends `command` to the server behind `name`, giving up after `timeout`.
pub fn client_at(name: &str, command: &str, timeout: Duration) -> Result<Reply, String> {
    exchange(name, command, timeout, false)
}
/// Like `client_at`, but allows the verified server to activate a requested UI.
pub fn client_at_foreground(name: &str, command: &str, timeout: Duration) -> Result<Reply, String> {
    exchange(name, command, timeout, true)
}
fn exchange(
    name: &str,
    command: &str,
    timeout: Duration,
    foreground: bool,
) -> Result<Reply, String> {
    // No detached blocking worker: each pending operation has a cancellation deadline.
    let deadline = Instant::now() + timeout;
    let wide_name = wide(name);
    unsafe {
        let _ = WaitNamedPipeW(
            PCWSTR(wide_name.as_ptr()),
            (timeout.as_millis() / 4).clamp(100, 3000) as u32,
        );
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED.0 | SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0)
        .open(name)
        .map_err(|e| format!("Winarchy unavailable: {e}"))?;
    let mut pid = 0;
    unsafe { GetNamedPipeServerProcessId(HANDLE(file.as_raw_handle()), &mut pid) }
        .map_err(|e| e.to_string())?;
    crate::identity::verify_server(pid)?;
    if foreground {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid) };
    }
    let mut io = PipeIo {
        handle: HANDLE(file.as_raw_handle()),
        deadline,
    };
    io.write_all(format!("{command}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let reply = {
        let mut reader = BufReader::new(&mut io);
        let mut bytes = Vec::new();
        loop {
            let mut b = [0];
            reader.read_exact(&mut b).map_err(|e| {
                format!("IPC response incomplete; command outcome may be unknown: {e}")
            })?;
            if b[0] == b'\n' {
                break;
            }
            if bytes.len() == MAX_REPLY {
                return Err("oversized IPC reply".into());
            }
            bytes.push(b[0]);
        }
        serde_json::from_slice::<Reply>(&bytes).map_err(|e| e.to_string())?
    };
    // Acknowledgement replaces unbounded FlushFileBuffers on the server. The
    // response is already known, so acknowledgement failure must not imply retry.
    let _ = io.write_all(&[ACK]);
    Ok(reply)
}
