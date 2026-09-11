use super::{
    Event, EventSender,
    native::wide,
    pipe_io::{self, PipeIo},
};
use crate::{
    command::Reply,
    protocol::read_command,
    request::{ReplyTo, Ticket},
};
use std::{
    io::{BufReader, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    sync::Arc,
    time::{Duration, Instant},
};
use winarchy_ipc::client::MAX_REPLY;
pub use winarchy_ipc::client::{client, pipe_name};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::Pipes::*,
    },
    core::PCWSTR,
};
fn create_pipe() -> Result<std::fs::File, String> {
    unsafe {
        let name = wide(&pipe_name()?);
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
fn dispatch(tx: &EventSender, command: crate::command::Command) -> Result<String, String> {
    let (sender, rx) = std::sync::mpsc::channel();
    let ticket = Arc::new(Ticket::new(Instant::now() + Duration::from_secs(5)));
    tx.send(Event::Command(
        command,
        Some(ReplyTo {
            ticket: ticket.clone(),
            sender,
        }),
    ))
    .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result,
        Err(_) if ticket.cancel() => Err("IPC request cancelled before execution".into()),
        Err(_) => {
            Err("IPC execution already started; outcome unknown, do not retry blindly".into())
        }
    }
}
fn serve(file: &std::fs::File, tx: &EventSender) -> Result<(), String> {
    let mut io = PipeIo {
        handle: HANDLE(file.as_raw_handle()),
        deadline: Instant::now() + Duration::from_secs(3),
    };
    let result =
        read_command(&mut BufReader::new(&mut io)).and_then(|command| dispatch(tx, command));
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
pub fn start(tx: EventSender) -> Result<(), String> {
    let file = create_pipe()?;
    std::thread::spawn(move || {
        let h = HANDLE(file.as_raw_handle());
        loop {
            if let Err(e) = pipe_io::connect(h) {
                unsafe {
                    let _ = DisconnectNamedPipe(h);
                }
                tracing::debug!(%e,"IPC connect interrupted");
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            if let Err(e) = serve(&file, &tx) {
                tracing::debug!(%e,"IPC client disconnected or timed out");
            }
            unsafe {
                let _ = DisconnectNamedPipe(h);
            }
        }
    });
    Ok(())
}
