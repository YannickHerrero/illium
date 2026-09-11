use super::{native, security};
use std::{
    os::windows::process::CommandExt,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::CoCreateGuid, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::{Owned, PCWSTR},
};
static READY: AtomicBool = AtomicBool::new(false);
static IDENTITY: OnceLock<String> = OnceLock::new();
static PROPERTY: OnceLock<Vec<u16>> = OnceLock::new();
static GENERATION: AtomicUsize = AtomicUsize::new(1);
fn initialize(identity: String) -> Result<(), String> {
    if identity.len() != 36
        || !identity.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("invalid recovery session identity".into());
    }
    PROPERTY
        .set(native::wide(&format!("WinarchySession-{identity}")))
        .map_err(|_| "session already initialized")?;
    IDENTITY
        .set(identity)
        .map_err(|_| "session already initialized")?;
    Ok(())
}
#[cfg(test)]
pub(super) fn test_initialize() -> Result<(), String> {
    initialize(format!(
        "{:?}",
        unsafe { CoCreateGuid() }.map_err(|e| e.to_string())?
    ))
}
fn marker() -> Option<std::path::PathBuf> {
    IDENTITY
        .get()
        .map(|id| crate::config::Config::home().join(format!("session-{id}.explorer")))
}
fn creation_time(h: HANDLE) -> Result<u64, String> {
    unsafe {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|e| e.to_string())?;
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}
pub fn tag(id: isize) -> Result<usize, String> {
    let key = PROPERTY
        .get()
        .ok_or("recovery session is not initialized")?;
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    if generation == 0 {
        return Err("window generation exhausted".into());
    }
    unsafe {
        SetPropW(
            native::hwnd(id),
            PCWSTR(key.as_ptr()),
            Some(HANDLE(generation as *mut _)),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(generation)
}
pub fn owns(id: isize, generation: usize) -> bool {
    generation != 0
        && PROPERTY.get().is_some_and(|key| unsafe {
            GetPropW(native::hwnd(id), PCWSTR(key.as_ptr())).0 as usize == generation
        })
}
pub fn untag(id: isize) {
    if let Some(key) = PROPERTY.get() {
        unsafe {
            let _ = RemovePropW(native::hwnd(id), PCWSTR(key.as_ptr()));
        }
    }
}
pub fn explorer(start: bool) -> Result<(), String> {
    if start {
        start_explorer()?;
        if let Some(path) = marker() {
            let _ = std::fs::remove_file(path);
        }
        return Ok(());
    }
    if !READY.load(Ordering::Acquire) {
        return Err("recovery watchdog is not ready; Explorer was not stopped".into());
    }
    let path = marker().ok_or("recovery session is not initialized")?;
    std::fs::write(path, b"restore Explorer").map_err(|e| e.to_string())?;
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
fn start_explorer() -> Result<(), String> {
    std::process::Command::new(security::os_executable("explorer.exe", false)?)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub struct Recovery;
impl Recovery {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let identity = format!("{:?}", CoCreateGuid().map_err(|e| e.to_string())?);
            initialize(identity.clone())?;
            let name = native::wide(&format!("Local\\WinarchyRecovery-{identity}"));
            let ready = Owned::new(
                CreateEventW(None, true, false, PCWSTR(name.as_ptr()))
                    .map_err(|e| e.to_string())?,
            );
            if GetLastError() == ERROR_ALREADY_EXISTS {
                return Err("recovery event identity collision".into());
            }
            let started = creation_time(GetCurrentProcess())?;
            std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                .args([
                    "--watch-session",
                    &std::process::id().to_string(),
                    &identity,
                    &started.to_string(),
                ])
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
            if WaitForSingleObject(*ready, 15000) != WAIT_OBJECT_0 {
                return Err("recovery watchdog did not initialize".into());
            }
            READY.store(true, Ordering::Release);
            Ok(Self)
        }
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if marker().is_some_and(|p| p.exists()) {
            let _ = explorer(true);
        }
        READY.store(false, Ordering::Release);
    }
}
pub fn watchdog(pid: u32, identity: &str, started: u64) -> Result<(), String> {
    security::require_standard_user()?;
    initialize(identity.to_owned())?;
    unsafe {
        let process = Owned::new(
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
            .map_err(|e| e.to_string())?,
        );
        if creation_time(*process)? != started {
            return Err("daemon process identity changed".into());
        }
        let name = native::wide(&format!("Local\\WinarchyRecovery-{identity}"));
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
        if let Some(key) = PROPERTY.get() {
            for id in native::enumerate() {
                if !GetPropW(native::hwnd(id), PCWSTR(key.as_ptr())).is_invalid() {
                    native::show(id, true);
                    untag(id);
                }
            }
        }
        if let Some(path) = marker()
            && path.exists()
        {
            start_explorer()?;
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}
