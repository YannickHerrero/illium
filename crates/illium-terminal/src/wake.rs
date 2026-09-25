//! A kernel event keeps PTY wakeups alive across Win32 modal message loops.
//! Unlike PostThreadMessage, dispatching unrelated messages cannot consume it.
use windows::{
    Win32::{Foundation::*, System::Threading::*},
    core::Result,
};

pub struct WakeEvent(HANDLE);
// Event handles support concurrent SetEvent/waits. Arc ownership keeps the
// handle open until both the UI and every PTY worker have released it.
unsafe impl Send for WakeEvent {}
unsafe impl Sync for WakeEvent {}

impl WakeEvent {
    pub fn new() -> Result<Self> {
        unsafe { CreateEventW(None, false, false, None).map(Self) }
    }

    pub fn handle(&self) -> HANDLE {
        self.0
    }

    pub fn signal(&self) -> Result<()> {
        unsafe { SetEvent(self.0) }
    }
}

impl Drop for WakeEvent {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use windows::Win32::UI::WindowsAndMessaging::*;

    #[test]
    fn output_wake_survives_message_dispatch_without_keyboard_input() {
        let event = Arc::new(WakeEvent::new().unwrap());
        unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            PostThreadMessageW(GetCurrentThreadId(), WM_APP + 42, WPARAM(0), LPARAM(0)).unwrap();
            let worker = event.clone();
            std::thread::spawn(move || {
                worker.signal().unwrap();
                worker.signal().unwrap();
            })
            .join()
            .unwrap();
            // Simulate a nested/modal loop consuming thread messages without
            // running the terminal's outer message handler.
            while PeekMessageW(&mut msg, None, WM_APP + 42, WM_APP + 42, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
            assert_eq!(
                MsgWaitForMultipleObjectsEx(
                    Some(&[event.handle()]),
                    0,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                ),
                WAIT_OBJECT_0,
            );
            // Auto-reset coalesces output bursts; no idle polling is required.
            assert_eq!(WaitForSingleObject(event.handle(), 0), WAIT_TIMEOUT);
            event.signal().unwrap();
            assert_eq!(WaitForSingleObject(event.handle(), 0), WAIT_OBJECT_0);
        }
    }
}
