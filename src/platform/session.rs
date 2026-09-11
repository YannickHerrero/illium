use super::{native, security};
use std::os::windows::process::CommandExt;
use windows::{
    Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*},
    core::{Owned, PCWSTR},
};
static READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
        start_explorer()?;
        let _ = std::fs::remove_file(marker(std::process::id()));
        Ok(())
    } else {
        if !READY.load(std::sync::atomic::Ordering::Acquire) {
            return Err("recovery watchdog is not ready; Explorer was not stopped".into());
        }
        // Record intent first, so a crash during taskkill is recoverable too.
        std::fs::write(marker(std::process::id()), b"restore Explorer")
            .map_err(|e| e.to_string())?;
        let taskkill = security::os_executable("taskkill.exe", true)?;
        let mut session = 0;
        unsafe {
            windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                std::process::id(),
                &mut session,
            )
        }
        .map_err(|e| e.to_string())?;
        let status = std::process::Command::new(taskkill)
            .args([
                "/IM",
                "explorer.exe",
                "/FI",
                &format!("SESSION eq {session}"),
                "/F",
            ])
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
fn start_explorer() -> Result<(), String> {
    let path = security::os_executable("explorer.exe", false)?;
    std::process::Command::new(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub struct Recovery;
impl Recovery {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let name = native::wide(&format!("Local\\WinarchyRecovery-{}", std::process::id()));
            let ready = Owned::new(
                CreateEventW(None, true, false, PCWSTR(name.as_ptr()))
                    .map_err(|e| e.to_string())?,
            );
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let result = std::process::Command::new(exe)
                .args(["--watch-session", &std::process::id().to_string()])
                .creation_flags(0x08000000)
                .spawn();
            let result = result.map_err(|e| e.to_string()).and_then(|_| {
                if WaitForSingleObject(*ready, 15000) == WAIT_OBJECT_0 {
                    Ok(())
                } else {
                    Err("recovery watchdog did not initialize".into())
                }
            });
            result?;
        }
        READY.store(true, std::sync::atomic::Ordering::Release);
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
    security::require_standard_user()?;
    unsafe {
        let process =
            Owned::new(OpenProcess(PROCESS_SYNCHRONIZE, false, pid).map_err(|e| e.to_string())?);
        let name = native::wide(&format!("Local\\WinarchyRecovery-{pid}"));
        let ready = Owned::new(
            OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr()))
                .map_err(|e| e.to_string())?,
        );
        SetEvent(*ready).map_err(|e| e.to_string())?;
        drop(ready);
        if WaitForSingleObject(*process, INFINITE) != WAIT_OBJECT_0 {
            return Err("could not wait for daemon exit".into());
        }
        drop(process);
        let property = property();
        for id in native::enumerate() {
            if GetPropW(native::hwnd(id), PCWSTR(property.as_ptr())).0 as usize == pid as usize {
                native::show(id, true);
                untag(id);
            }
        }
        if marker(pid).exists() {
            start_explorer()?;
            let _ = std::fs::remove_file(marker(pid));
        }
        Ok(())
    }
}
