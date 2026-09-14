//! Endpoint identity comes from the Windows token/session, never USERNAME.
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        System::{RemoteDesktop::*, Threading::*},
    },
    core::PWSTR,
};
fn user_sid(process: HANDLE) -> Result<String, String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(|e| e.to_string())?;
        let token = windows::core::Owned::new(token);
        let mut bytes = 0;
        let _ = GetTokenInformation(*token, TokenUser, None, 0, &mut bytes);
        if bytes == 0 || bytes > 65536 {
            return Err("invalid token information size".into());
        }
        // TOKEN_USER contains pointers; use an aligned buffer, not Vec<u8>.
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        GetTokenInformation(
            *token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            bytes,
            &mut bytes,
        )
        .map_err(|e| e.to_string())?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(|e| e.to_string())?;
        let result = text.to_string().map_err(|e| e.to_string());
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        result
    }
}
fn session(pid: u32) -> Result<u32, String> {
    let mut id = 0;
    unsafe { ProcessIdToSessionId(pid, &mut id) }.map_err(|e| e.to_string())?;
    Ok(id)
}
pub fn endpoint() -> Result<String, String> {
    endpoint_named("winarchy")
}
/// `<prefix>-<user SID>-<session>`: one endpoint per server kind, user and session.
pub fn endpoint_named(prefix: &str) -> Result<String, String> {
    Ok(format!(
        "{prefix}-{}-{}",
        user_sid(unsafe { GetCurrentProcess() })?,
        session(std::process::id())?
    ))
}
pub fn verify_server(pid: u32) -> Result<(), String> {
    unsafe {
        if session(pid)? != session(std::process::id())? {
            return Err("IPC server belongs to a different Windows session".into());
        }
        let process = windows::core::Owned::new(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|e| e.to_string())?,
        );
        if user_sid(*process)? != user_sid(GetCurrentProcess())? {
            return Err("IPC server belongs to a different Windows user".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn own_process_identity_matches() {
        verify_server(std::process::id()).unwrap();
        assert!(endpoint().unwrap().starts_with("winarchy-S-1-"));
    }
}
