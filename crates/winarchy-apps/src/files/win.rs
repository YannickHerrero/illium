//! Disk operations through the shell: IFileOperation for copy, move and
//! delete (recycle bin, progress and conflict dialogs come from Windows),
//! ShellExecute to open files.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        System::Com::*,
        UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{HSTRING, PCWSTR},
};
fn item(path: &Path) -> Result<IShellItem, String> {
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
        .map_err(|e| format!("{}: {}", path.display(), e.message()))
}
fn operation(flags: FILEOPERATION_FLAGS) -> Result<IFileOperation, String> {
    unsafe {
        let op: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message())?;
        op.SetOperationFlags(flags).map_err(|e| e.message())?;
        Ok(op)
    }
}
fn perform(op: &IFileOperation) -> Result<(), String> {
    unsafe {
        op.PerformOperations().map_err(|e| e.message())?;
        if op
            .GetAnyOperationsAborted()
            .map_err(|e| e.message())?
            .as_bool()
        {
            return Err("cancelled".into());
        }
    }
    Ok(())
}
pub fn copy(sources: &[PathBuf], into: &Path, cut: bool) -> Result<(), String> {
    let op = operation(FOF_ALLOWUNDO | FOF_NOCONFIRMMKDIR)?;
    let dest = item(into)?;
    for source in sources {
        let src = item(source)?;
        unsafe {
            if cut {
                op.MoveItem(&src, &dest, PCWSTR::null(), None)
            } else {
                op.CopyItem(&src, &dest, PCWSTR::null(), None)
            }
        }
        .map_err(|e| e.message())?;
    }
    perform(&op)
}
/// To the recycle bin when `recycle`, otherwise permanently after the
/// system's own confirmation.
pub fn delete(paths: &[PathBuf], recycle: bool) -> Result<(), String> {
    let op = operation(if recycle {
        FOF_ALLOWUNDO | FOFX_ADDUNDORECORD
    } else {
        FOF_WANTNUKEWARNING
    })?;
    for path in paths {
        unsafe { op.DeleteItem(&item(path)?, None) }.map_err(|e| e.message())?;
    }
    perform(&op)
}
pub fn open(path: &Path) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            &HSTRING::from(path.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values up to 32 are error codes, as documented for ShellExecute.
    if result.0 as usize <= 32 {
        return Err(format!("cannot open {}", path.display()));
    }
    Ok(())
}
/// The `terminal` alias of `apps.toml`, started in `dir`.
pub fn terminal(home: &Path, dir: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(home.join("apps.toml")).map_err(|e| e.to_string())?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| format!("apps.toml: {e}"))?;
    let alias = table
        .get("apps")
        .and_then(|a| a.get("terminal"))
        .and_then(|t| t.as_str())
        .ok_or("apps.toml has no terminal alias")?;
    let mut parts = alias.split_whitespace();
    let program = parts.next().ok_or("empty terminal alias")?;
    std::process::Command::new(program)
        .args(parts)
        .current_dir(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{program}: {e}"))
}
