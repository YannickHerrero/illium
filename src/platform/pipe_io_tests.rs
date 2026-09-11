//! Native pipe tests: no hooks, application windows or Explorer operations.
use super::{native::wide, pipe_io::PipeIo};
use std::{
    io::{Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, Storage::FileSystem::*, System::Pipes::*},
    core::PCWSTR,
};
fn pair() -> (std::fs::File, std::fs::File) {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let name = format!(
        r"\\.\pipe\winarchy-io-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let w = wide(&name);
    let h = unsafe {
        CreateNamedPipeW(
            PCWSTR(w.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            1000,
            None,
        )
    };
    assert_ne!(h, INVALID_HANDLE_VALUE);
    let server = unsafe { std::fs::File::from_raw_handle(h.0) };
    let client = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED.0 | SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0)
        .open(name)
        .unwrap();
    super::pipe_io::connect(h).unwrap();
    (server, client)
}
#[test]
fn silent_peer_times_out_and_cancelled_read_does_not_steal_bytes() {
    let (server, client) = pair();
    let start = Instant::now();
    let mut input = PipeIo {
        handle: HANDLE(server.as_raw_handle()),
        deadline: start + Duration::from_millis(40),
    };
    assert_eq!(
        input.read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    let mut output = PipeIo {
        handle: HANDLE(client.as_raw_handle()),
        deadline: Instant::now() + Duration::from_secs(1),
    };
    output.write_all(b"X").unwrap();
    input.deadline = Instant::now() + Duration::from_secs(1);
    let mut byte = [0];
    input.read_exact(&mut byte).unwrap();
    assert_eq!(&byte, b"X");
}
#[test]
fn peer_not_reading_cannot_block_large_write_forever() {
    let (_server, client) = pair();
    let mut output = PipeIo {
        handle: HANDLE(client.as_raw_handle()),
        deadline: Instant::now() + Duration::from_millis(40),
    };
    assert_eq!(
        output.write_all(&vec![42; 128 * 1024]).unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
}
#[test]
fn disconnected_peer_is_an_error() {
    let (server, client) = pair();
    drop(client);
    let mut input = PipeIo {
        handle: HANDLE(server.as_raw_handle()),
        deadline: Instant::now() + Duration::from_secs(1),
    };
    assert!(input.read(&mut [0]).is_err());
}
