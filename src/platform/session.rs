use super::native;
use std::os::windows::process::CommandExt;
use windows::{
    Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*},
    core::PCWSTR,
};
fn marker(pid: u32) -> std::path::PathBuf {
    crate::config::Config::home().join(format!("session-{pid}.explorer"))
}
fn property() -> Vec<u16> {
    native::wide("WinarchySessionOwner")
}
pub fn tag(id: isize) {
    unsafe {
        let name = property();
        let _ = SetPropW(
            native::hwnd(id),
            PCWSTR(name.as_ptr()),
            Some(HANDLE(std::process::id() as usize as *mut _)),
        );
    }
}
pub fn untag(id: isize) {
    unsafe {
        let name = property();
        let _ = RemovePropW(native::hwnd(id), PCWSTR(name.as_ptr()));
    }
}
pub fn explorer(start: bool) -> Result<(), String> {
    if start {
        native::spawn("explorer.exe")?;
        let _ = std::fs::remove_file(marker(std::process::id()));
        Ok(())
    } else {
        // Record intent first, so a crash during taskkill is recoverable too.
        std::fs::write(marker(std::process::id()), b"restore Explorer")
            .map_err(|e| e.to_string())?;
        let status = std::process::Command::new("taskkill.exe")
            .args(["/IM", "explorer.exe", "/F"])
            .creation_flags(0x08000000)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("could not stop Explorer".into())
        }
    }
}
pub struct Recovery;
impl Recovery {
    pub fn new(replace: bool) -> Result<Self, String> {
        unsafe {
            let name = native::wide(&format!("Local\\WinarchyRecovery-{}", std::process::id()));
            let ready = CreateEventW(None, true, false, PCWSTR(name.as_ptr()))
                .map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let result = std::process::Command::new(exe)
                .args(["--watch-session", &std::process::id().to_string()])
                .creation_flags(0x08000000)
                .spawn();
            let result = result.map_err(|e| e.to_string()).and_then(|_| {
                if WaitForSingleObject(ready, 15000) == WAIT_OBJECT_0 {
                    Ok(())
                } else {
                    Err("recovery watchdog did not initialize".into())
                }
            });
            let _ = CloseHandle(ready);
            result?;
        }
        if replace {
            explorer(false)?;
        }
        Ok(Self)
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if marker(std::process::id()).exists() {
            let _ = explorer(true);
        }
    }
}
pub fn watchdog(pid: u32) -> Result<(), String> {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid).map_err(|e| e.to_string())?;
        let name = native::wide(&format!("Local\\WinarchyRecovery-{pid}"));
        let ready = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr()))
            .map_err(|e| e.to_string())?;
        let _ = SetEvent(ready);
        let _ = CloseHandle(ready);
        let _ = WaitForSingleObject(process, INFINITE);
        let _ = CloseHandle(process);
        let property = property();
        for id in native::enumerate() {
            if GetPropW(native::hwnd(id), PCWSTR(property.as_ptr())).0 as usize == pid as usize {
                native::show(id, true);
                untag(id);
            }
        }
        if marker(pid).exists() {
            native::spawn("explorer.exe")?;
            let _ = std::fs::remove_file(marker(pid));
        }
        Ok(())
    }
}
