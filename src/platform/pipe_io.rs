//! Overlapped named-pipe I/O with absolute deadlines and cancellation completion.
//! Buffers and OVERLAPPED stay alive until Windows has completed/cancelled the I/O.
use std::{
    io::{self, Read, Write},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{IO::*, Pipes::*, Threading::*},
    },
    core::{HRESULT, Owned},
};
fn error(e: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(e.code().0 & 0xffff)
}
fn operation(
    h: HANDLE,
    deadline: Option<Instant>,
    start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
) -> io::Result<usize> {
    if deadline.is_some_and(|d| Instant::now() >= d) {
        return Err(io::ErrorKind::TimedOut.into());
    }
    unsafe {
        let event = Owned::new(CreateEventW(None, true, false, None).map_err(error)?);
        let mut op = OVERLAPPED {
            hEvent: *event,
            ..Default::default()
        };
        match start(&mut op) {
            Ok(()) => {}
            Err(e) if e.code() == HRESULT::from_win32(ERROR_PIPE_CONNECTED.0) => return Ok(0),
            Err(e) if e.code() == HRESULT::from_win32(ERROR_IO_PENDING.0) => {
                let ms = deadline.map_or(INFINITE, |d| {
                    d.saturating_duration_since(Instant::now())
                        .as_millis()
                        .min((INFINITE - 1) as u128) as u32
                });
                let result = WaitForSingleObject(*event, ms);
                if result != WAIT_OBJECT_0 {
                    let failure = if result == WAIT_TIMEOUT {
                        io::Error::from(io::ErrorKind::TimedOut)
                    } else {
                        io::Error::last_os_error()
                    };
                    let _ = CancelIoEx(h, Some(&op));
                    // Required even if cancellation lost the race with completion.
                    let mut transferred = 0;
                    let _ = GetOverlappedResult(h, &op, &mut transferred, true);
                    return Err(failure);
                }
            }
            Err(e) => return Err(error(e)),
        }
        let mut transferred = 0;
        GetOverlappedResult(h, &op, &mut transferred, false).map_err(error)?;
        Ok(transferred as usize)
    }
}
pub fn connect(h: HANDLE) -> io::Result<()> {
    operation(h, None, |op| unsafe { ConnectNamedPipe(h, Some(op)) }).map(|_| ())
}
pub struct PipeIo {
    pub handle: HANDLE,
    pub deadline: Instant,
}
impl Read for PipeIo {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        operation(self.handle, Some(self.deadline), |op| unsafe {
            ReadFile(self.handle, Some(buffer), None, Some(op))
        })
    }
}
impl Write for PipeIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        operation(self.handle, Some(self.deadline), |op| unsafe {
            WriteFile(self.handle, Some(buffer), None, Some(op))
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
