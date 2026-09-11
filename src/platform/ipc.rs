use super::{Event, native::wide};
use crate::command::{Command, Reply};
use std::{
    io::{Read, Write},
    os::windows::io::FromRawHandle,
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
    let name = pipe_name();
    let wide_name = wide(&name);
    unsafe {
        let _ = WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), 3000);
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|e| format!("Winarchy unavailable: {e}"))?;
    file.write_all(format!("{command}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = String::new();
    let mut b = [0];
    while response.len() < 65536 && file.read(&mut b).map_err(|e| e.to_string())? == 1 {
        if b[0] == b'\n' {
            break;
        }
        response.push(b[0] as char);
    }
    serde_json::from_str(&response).map_err(|e| e.to_string())
}
pub fn start(tx: Sender<Event>) -> Result<(), String> {
    let (ready, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let name = wide(&pipe_name());
        // Owner-only access. Never expose command execution to other local users.
        let sddl = wide("D:P(A;;GA;;;OW)");
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        if let Err(e) = ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        ) {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
        let security = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut first = true;
        loop {
            let h = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX
                    | if first {
                        FILE_FLAG_FIRST_PIPE_INSTANCE
                    } else {
                        FILE_FLAGS_AND_ATTRIBUTES(0)
                    },
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                1000,
                Some(&security),
            );
            if h == INVALID_HANDLE_VALUE {
                let e = windows::core::Error::from_win32().to_string();
                let _ = ready.send(Err(e.clone()));
                tracing::error!(%e,"pipe creation failed");
                break;
            }
            if first {
                let _ = ready.send(Ok(()));
                first = false;
            }
            let connected =
                ConnectNamedPipe(h, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED;
            if connected {
                let mut file = std::fs::File::from_raw_handle(h.0);
                let mut bytes = Vec::new();
                let mut b = [0];
                while bytes.len() < 8192 {
                    match file.read(&mut b) {
                        Ok(1) if b[0] != b'\n' => bytes.push(b[0]),
                        _ => break,
                    }
                }
                let result = String::from_utf8(bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|s| s.parse::<Command>())
                    .and_then(|c| {
                        let (reply, rx) = std::sync::mpsc::channel();
                        tx.send(Event::Command(c, Some(reply)))
                            .map_err(|e| e.to_string())?;
                        rx.recv_timeout(std::time::Duration::from_secs(5))
                            .map_err(|e| e.to_string())?
                    });
                let reply = match result {
                    Ok(message) => Reply { ok: true, message },
                    Err(message) => Reply { ok: false, message },
                };
                let _ = writeln!(
                    file,
                    "{}",
                    serde_json::to_string(&reply).unwrap_or_default()
                );
                let _ = file.flush();
                let _ = FlushFileBuffers(h);
                let _ = DisconnectNamedPipe(h);
            } else {
                let _ = CloseHandle(h);
            }
        }
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    });
    rx.recv().map_err(|e| e.to_string())?
}
