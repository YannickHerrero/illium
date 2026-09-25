//! Process-level safety boundaries; none of these checks change system policy.
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::{
        SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW},
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

/// Illium intentionally does not provide an elevated command-execution broker.
pub fn require_standard_user() -> Result<(), String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| e.to_string())?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        let _ = CloseHandle(token);
        result.map_err(|e| e.to_string())?;
        if elevation.TokenIsElevated != 0 {
            return Err(
                "Do not run Illium as administrator; start it with a standard user token".into(),
            );
        }
        Ok(())
    }
}

/// Resolve OS-owned executables through Win32, not CWD, PATH, or environment variables.
pub fn os_executable(name: &str, system32: bool) -> Result<std::path::PathBuf, String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.') {
        return Err("invalid OS executable name".into());
    }
    let mut buffer = [0u16; 32768];
    let length = unsafe {
        if system32 {
            GetSystemDirectoryW(Some(&mut buffer))
        } else {
            GetWindowsDirectoryW(Some(&mut buffer))
        }
    } as usize;
    if length == 0 || length >= buffer.len() {
        return Err("cannot resolve Windows system directory".into());
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..length])).join(name))
}
