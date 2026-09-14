//! Owner-only local named-pipe server: one client at a time, bounded framing,
//! JSON reply, client acknowledgement. The daemon and the resident
//! applications process both serve this way; only the handler differs.
use crate::{client::MAX_REPLY, command::Reply, pipe_io::PipeIo, protocol::read_line, wide};
use std::{
    io::{BufReader, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    time::{Duration, Instant},
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
/// The single pipe instance at `\\.\pipe\<name>`; fails when one already exists.
pub fn create_pipe(name: &str) -> Result<std::fs::File, String> {
    unsafe {
        let name = wide(name);
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
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            8192,
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
/// Reads one command from the connected client, answers with the handler's
/// result and waits briefly for the acknowledgement byte.
pub fn serve_one(
    file: &std::fs::File,
    handler: impl FnOnce(String) -> Result<String, String>,
) -> Result<(), String> {
    let mut io = PipeIo {
        handle: HANDLE(file.as_raw_handle()),
        deadline: Instant::now() + Duration::from_secs(3),
    };
    let result = read_line(&mut BufReader::new(&mut io)).and_then(handler);
    let reply = match result {
        Ok(message) => Reply { ok: true, message },
        Err(message) => Reply { ok: false, message },
    };
    let mut json = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    if json.len() > MAX_REPLY {
        json = serde_json::to_vec(&Reply {
            ok: false,
            message: "IPC reply exceeds size limit".into(),
        })
        .map_err(|e| e.to_string())?;
    }
    json.push(b'\n');
    io.deadline = Instant::now() + Duration::from_secs(2);
    io.write_all(&json).map_err(|e| e.to_string())?;
    io.deadline = Instant::now() + Duration::from_secs(1);
    let mut ack = [0];
    let _ = io.read_exact(&mut ack); // Old clients may close without ACK; wait is bounded.
    Ok(())
}
/// Accepts clients forever on the current thread, one after the other.
/// `on_error` sees connection and framing failures, which are routine.
pub fn accept_loop(
    file: &std::fs::File,
    mut handler: impl FnMut(String) -> Result<String, String>,
    on_error: impl Fn(&str),
) {
    let h = HANDLE(file.as_raw_handle());
    loop {
        if let Err(e) = crate::pipe_io::connect(h) {
            unsafe {
                let _ = DisconnectNamedPipe(h);
            }
            on_error(&format!("IPC connect interrupted: {e}"));
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if let Err(e) = serve_one(file, &mut handler) {
            on_error(&format!("IPC client disconnected or timed out: {e}"));
        }
        unsafe {
            let _ = DisconnectNamedPipe(h);
        }
    }
}
