use super::native::wide;
use windows::{
    Win32::{Foundation::*, System::Threading::*},
    core::PCWSTR,
};
/// Held for the entire session, including UI initialization and shutdown.
pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire() -> Result<Self, String> {
        unsafe {
            let name = wide(&format!("Local\\{}", super::identity::endpoint()?));
            let h = CreateMutexW(None, false, PCWSTR(name.as_ptr())).map_err(|e| e.to_string())?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(h);
                return Err("Illium is already running".into());
            }
            Ok(Self(h))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
