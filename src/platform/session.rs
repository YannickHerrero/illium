use super::native;
use std::os::windows::process::CommandExt;
use windows::Win32::{Foundation::CloseHandle, System::Threading::*};
pub fn explorer(start: bool) -> Result<(), String> {
    if start {
        native::spawn("explorer.exe")
    } else {
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
pub struct Recovery(bool);
impl Recovery {
    pub fn new(replace: bool) -> Result<Self, String> {
        if replace {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            std::process::Command::new(exe)
                .args(["--watch-session", &std::process::id().to_string()])
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
            explorer(false)?;
        }
        Ok(Self(replace))
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if self.0 {
            let _ = explorer(true);
        }
    }
}
pub fn watchdog(pid: u32) -> Result<(), String> {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
        explorer(true)
    }
}
