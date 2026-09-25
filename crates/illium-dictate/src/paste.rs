//! Puts the transcript into the focused text field: it goes through the
//! clipboard and an injected Ctrl+V, then the previous clipboard text comes
//! back unless something else wrote to the clipboard meanwhile.
use windows::Win32::{
    Foundation::{HANDLE, HGLOBAL},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
            IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
    UI::Input::KeyboardAndMouse::*,
};
const CF_UNICODETEXT: u32 = 13;
/// Same marker as the daemon's own injected input, so its keyboard hook
/// ignores the paste chord.
const INJECTED: usize = 0x5741_5243;
struct Clipboard;
impl Clipboard {
    fn open() -> Result<Self, String> {
        for _ in 0..25 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err("clipboard is busy".into())
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}
fn read_text() -> Option<String> {
    unsafe {
        let _clipboard = Clipboard::open().ok()?;
        IsClipboardFormatAvailable(CF_UNICODETEXT).ok()?;
        let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
        let global = HGLOBAL(handle.0);
        let data = GlobalLock(global) as *const u16;
        if data.is_null() {
            return None;
        }
        let mut len = 0;
        while *data.add(len) != 0 && len < 1 << 22 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(data, len));
        let _ = GlobalUnlock(global);
        Some(text)
    }
}
fn write_text(text: &str) -> Result<(), String> {
    unsafe {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let _clipboard = Clipboard::open()?;
        EmptyClipboard().map_err(|e| e.to_string())?;
        let global = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| e.to_string())?;
        let target = GlobalLock(global) as *mut u16;
        if target.is_null() {
            return Err("clipboard allocation failed".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
        let _ = GlobalUnlock(global);
        // The clipboard owns the memory after a successful SetClipboardData.
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(global.0))).map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: INJECTED,
            },
        },
    }
}
pub fn paste(text: &str) -> Result<(), String> {
    let previous = read_text();
    write_text(text)?;
    let sequence = unsafe { GetClipboardSequenceNumber() };
    let chord = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&chord, std::mem::size_of::<INPUT>() as i32) };
    if sent != chord.len() as u32 {
        return Err("paste keystrokes were blocked".into());
    }
    // Give the target time to read the clipboard before the old text returns.
    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Some(previous) = previous
        && unsafe { GetClipboardSequenceNumber() } == sequence
    {
        let _ = write_text(&previous);
    }
    Ok(())
}
