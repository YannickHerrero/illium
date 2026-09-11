use super::{Event, native::wide};
use crate::{command::Reply, protocol::read_command};
use std::{
    io::{Read, Write},
    os::windows::{fs::OpenOptionsExt, io::FromRawHandle},
    sync::mpsc::Sender,
};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::Pipes::*,
    },
    core::PCWSTR,
};
pub fn pipe_name() -> String {
    format!(
        r"\\.\pipe\winarchy-{}",
        std::env::var("USERNAME").unwrap_or_default()
    )
}
pub fn client(command: &str) -> Result<Reply, String> {
    let command = command.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(exchange(&command));
    });
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "Winarchy IPC timed out".to_owned())?
}
fn exchange(command: &str) -> Result<Reply, String> {
    let name = pipe_name();
    let wide_name = wide(&name);
    unsafe {
        let _ = WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), 3000);
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        // A pre-created hostile pipe must never be able to impersonate this client.
        .custom_flags(SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0)
        .open(name)
        .map_err(|e| format!("Winarchy unavailable: {e}"))?;
    file.write_all(format!("{command}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    let mut b = [0];
    while response.len() < 65536 && file.read(&mut b).map_err(|e| e.to_string())? == 1 {
        if b[0] == b'\n' {
            return serde_json::from_slice(&response).map_err(|e| e.to_string());
        }
        response.push(b[0]);
    }
    Err("incomplete or oversized IPC reply".into())
}
/// The File owns the single pipe instance for the full server lifetime, including
/// between clients. Closing/recreating it would permit pipe-name takeover races.
fn create_pipe() -> Result<std::fs::File, String> {
    unsafe {
        let name = wide(&pipe_name());
        let sddl = wide("D:P(A;;GA;;;OW)");
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(|e| e.to_string())?;
        let security = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let h = CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            1000,
            Some(&security),
        );
        let error = (h == INVALID_HANDLE_VALUE).then(windows::core::Error::from_win32);
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if let Some(e) = error {
            return Err(e.to_string());
        }
        Ok(std::fs::File::from_raw_handle(h.0))
    }
}
pub fn start(tx: Sender<Event>) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    // Fail synchronously before any UI/window management on name/DACL errors.
    let mut file = create_pipe()?;
    std::thread::spawn(move || unsafe {
        let h = HANDLE(file.as_raw_handle());
        loop {
            if ConnectNamedPipe(h, None).is_err() && GetLastError() != ERROR_PIPE_CONNECTED {
                tracing::error!(error = %windows::core::Error::from_win32(), "IPC listener stopped");
                break;
            }
            let result = read_command(&mut file).and_then(|c| {
                let (reply, rx) = std::sync::mpsc::channel();
                let ticket = std::sync::Arc::new(crate::request::Ticket::new(
                    std::time::Instant::now() + std::time::Duration::from_secs(5),
                ));
                tx.send(Event::Command(
                    c,
                    Some(crate::request::ReplyTo {
                        ticket: ticket.clone(),
                        sender: reply,
                    }),
                ))
                .map_err(|e| e.to_string())?;
                match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                    Ok(result) => result,
                    Err(_) if ticket.cancel() => {
                        Err("IPC request cancelled before execution".into())
                    }
                    Err(_) => Err(
                        "IPC execution already started; outcome unknown, do not retry blindly"
                            .into(),
                    ),
                }
            });
            let reply = match result {
                Ok(message) => Reply { ok: true, message },
                Err(message) => Reply { ok: false, message },
            };
            if let Ok(json) = serde_json::to_string(&reply) {
                let _ = writeln!(file, "{json}");
            }
            let _ = FlushFileBuffers(h);
            let _ = DisconnectNamedPipe(h);
        }
    });
    Ok(())
}
