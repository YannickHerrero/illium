//! Disk operations through the shell: IFileOperation for copy, move and
//! delete (recycle bin, progress and conflict dialogs come from Windows),
//! ShellExecute to open files.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Storage::FileSystem::GetLogicalDrives,
        System::{Com::*, Registry::*},
        UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{HSTRING, PCWSTR, PWSTR},
};
const LXSS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Lxss";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn utf16(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}
/// A REG_SZ value under `key\\sub`, when present.
fn reg_string(key: HKEY, sub: Option<&str>, value: &str) -> Option<String> {
    let sub = sub.map(wide);
    let value = wide(value);
    let mut buffer = [0u16; 512];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            key,
            sub.as_ref().map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr())),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .is_ok()
    .then(|| utf16(&buffer))
}
/// Registered WSL distributions as (name, is default). The `\\wsl.localhost\`
/// root cannot be enumerated, the registry is the only list.
fn wsl_distributions() -> Vec<(String, bool)> {
    let mut out = Vec::new();
    unsafe {
        let path = wide(LXSS);
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        )
        .is_err()
        {
            return out;
        }
        let default = reg_string(key, None, "DefaultDistribution").unwrap_or_default();
        let mut index = 0;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            index += 1;
            let guid = utf16(&name);
            if let Some(distribution) = reg_string(key, Some(&guid), "DistributionName") {
                out.push((distribution, guid == default));
            }
        }
        let _ = RegCloseKey(key);
    }
    out
}
/// A REG_DWORD value under `key\\sub`, when present.
fn reg_dword(key: HKEY, sub: &str, value: &str) -> Option<u32> {
    let sub = wide(sub);
    let value = wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            key,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    }
    .is_ok()
    .then_some(data)
}
/// Home of the default distribution's default user: its uid comes from the
/// registry and the directory from the distribution's own /etc/passwd.
pub fn default_wsl_home() -> Option<PathBuf> {
    let path = wide(LXSS);
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        )
    }
    .ok()
    .ok()?;
    let default = reg_string(key, None, "DefaultDistribution");
    let result = default.and_then(|guid| {
        let name = reg_string(key, Some(&guid), "DistributionName")?;
        let uid = reg_dword(key, &guid, "DefaultUid").unwrap_or(0);
        let root = wsl_path(&name);
        let passwd = std::fs::read_to_string(root.join("etc").join("passwd")).ok()?;
        let home = passwd
            .lines()
            .map(|l| l.split(':').collect::<Vec<_>>())
            .find(|f| f.len() >= 6 && f[2] == uid.to_string())
            .map(|f| f[5].trim_start_matches('/').replace('/', "\\"))
            .filter(|h| !h.is_empty())?;
        Some(root.join(home))
    });
    unsafe {
        let _ = RegCloseKey(key);
    }
    result.or_else(default_wsl_root)
}
fn wsl_path(distribution: &str) -> PathBuf {
    PathBuf::from(format!("\\\\wsl.localhost\\{distribution}\\"))
}
pub fn roots() -> Vec<(String, PathBuf)> {
    let mask = unsafe { GetLogicalDrives() };
    let mut roots: Vec<(String, PathBuf)> = (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            let drive = format!("{}:\\", (b'A' + i as u8) as char);
            (drive.clone(), PathBuf::from(drive))
        })
        .collect();
    for (name, _) in wsl_distributions() {
        roots.push((format!("wsl: {name}"), wsl_path(&name)));
    }
    roots
}
pub fn default_wsl_root() -> Option<PathBuf> {
    let distributions = wsl_distributions();
    distributions
        .iter()
        .find(|(_, default)| *default)
        .or(distributions.first())
        .map(|(name, _)| wsl_path(name))
}
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
