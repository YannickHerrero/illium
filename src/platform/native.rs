use crate::layout::Rect;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::Threading::*,
        UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, PWSTR},
};
/// `dwExtraInfo` marker on input the daemon injects, so its own keyboard hook
/// leaves modifier tracking untouched.
pub const INJECTED: usize = 0x5741_5243;
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn hwnd(id: isize) -> HWND {
    HWND(id as *mut _)
}
pub fn rect(id: isize) -> Rect {
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd(id), &mut r);
        Rect {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        }
    }
}
pub fn visible(id: isize) -> bool {
    unsafe { IsWindowVisible(hwnd(id)).as_bool() }
}
pub fn minimized(id: isize) -> bool {
    unsafe { IsIconic(hwnd(id)).as_bool() }
}
pub fn title(id: isize) -> String {
    unsafe {
        let mut s = [0u16; 1024];
        let n = GetWindowTextW(hwnd(id), &mut s);
        String::from_utf16_lossy(&s[..n.max(0) as usize])
    }
}
pub fn metadata(id: isize) -> Option<(String, String, bool)> {
    unsafe {
        let h = hwnd(id);
        if !IsWindow(Some(h)).as_bool() || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        if style & WS_CHILD.0 != 0 || ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
        if cloaked != 0 {
            return None;
        }
        let mut class = [0u16; 256];
        let n = GetClassNameW(h, &mut class);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if [
            "#32768",
            "tooltips_class32",
            "XamlExplorerHostIslandWindow",
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "Windows.UI.Core.CoreWindow",
            "ApplicationManager_DesktopShellWindow",
        ]
        .contains(&class.as_str())
        {
            return None;
        }
        let mut exe = String::new();
        if let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 2048];
            let mut n = buf.len() as u32;
            if QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n)
                .is_ok()
            {
                exe = String::from_utf16_lossy(&buf[..n as usize]);
            }
            let _ = CloseHandle(p);
        }
        let floating = class == "#32770" || GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid());
        Some((exe, class, floating))
    }
}
pub fn enumerate() -> Vec<isize> {
    unsafe extern "system" fn callback(h: HWND, p: LPARAM) -> BOOL {
        unsafe {
            (*(p.0 as *mut Vec<isize>)).push(h.0 as isize);
        }
        BOOL(1)
    }
    let mut ids = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut ids as *mut _ as isize));
    }
    ids
}
pub fn monitors() -> Vec<Rect> {
    unsafe extern "system" fn callback(_: HMONITOR, _: HDC, r: *mut RECT, p: LPARAM) -> BOOL {
        unsafe {
            let r = *r;
            (*(p.0 as *mut Vec<Rect>)).push(Rect {
                x: r.left,
                y: r.top,
                w: r.right - r.left,
                h: r.bottom - r.top,
            });
        }
        BOOL(1)
    }
    let mut out = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(callback),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    out
}
pub fn show(id: isize, visible: bool) {
    unsafe {
        let _ = ShowWindow(hwnd(id), if visible { SW_SHOWNA } else { SW_HIDE });
    }
}
/// Zero-size activatable window that takes the foreground when no client can,
/// so keystrokes never land in a window Winarchy just hid. Without Explorer
/// nothing else picks up activation from a hidden foreground window.
pub fn sink() -> isize {
    unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    thread_local! {
        static SINK: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    }
    SINK.with(|sink| {
        if sink.get() == 0 {
            unsafe {
                let class = wide("WinarchyFocusSink");
                let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                    .unwrap_or_default();
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: PCWSTR(class.as_ptr()),
                    ..Default::default()
                });
                if let Ok(h) = CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    PCWSTR(class.as_ptr()),
                    PCWSTR(class.as_ptr()),
                    WS_POPUP | WS_VISIBLE,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                ) {
                    sink.set(h.0 as isize);
                }
            }
        }
        sink.get()
    })
}
pub fn focus(id: isize) {
    if id == 0 {
        return;
    }
    unsafe {
        if IsIconic(hwnd(id)).as_bool() {
            let _ = ShowWindow(hwnd(id), SW_RESTORE);
        }
        // Sharing the foreground thread's input queue with AttachThreadInput made
        // Windows reject the request on a host with an unbounded foreground lock,
        // while a plain call succeeded. Injecting a key release makes this
        // process the last input source, which is one of the allowed cases.
        if !SetForegroundWindow(hwnd(id)).as_bool() {
            let release = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        dwFlags: KEYEVENTF_KEYUP,
                        dwExtraInfo: INJECTED,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[release], std::mem::size_of::<INPUT>() as i32);
            if !SetForegroundWindow(hwnd(id)).as_bool() {
                tracing::debug!(id, "foreground request rejected");
            }
        }
    }
}
pub fn close(id: isize) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(id)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
pub fn position(id: isize, r: Rect, layer: Option<HWND>) {
    if id == 0 {
        return;
    }
    unsafe {
        if let Err(e) = SetWindowPos(
            hwnd(id),
            layer,
            r.x,
            r.y,
            r.w,
            r.h,
            SWP_NOACTIVATE
                | if layer.is_none() {
                    SWP_NOZORDER
                } else {
                    SET_WINDOW_POS_FLAGS(0)
                },
        ) {
            tracing::debug!(%e,id,"position failed");
        }
    }
}
pub fn batch(items: &[(isize, Rect)]) {
    unsafe {
        let Ok(mut d) = BeginDeferWindowPos(items.len() as i32) else {
            return;
        };
        for (id, r) in items {
            match DeferWindowPos(
                d,
                hwnd(*id),
                None,
                r.x,
                r.y,
                r.w,
                r.h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) {
                Ok(next) => d = next,
                Err(e) => {
                    tracing::debug!(%e,"defer failed");
                    return;
                }
            }
        }
        let _ = EndDeferWindowPos(d);
    }
}
pub fn spawn(command: &str) -> Result<(), String> {
    unsafe {
        let mut line = wide(command);
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        let created = CreateProcessW(
            None,
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            None,
            None,
            &si,
            &mut pi,
        );
        if let Err(e) = created {
            // Bare aliases may be registered in Windows App Paths rather than PATH.
            if !command.chars().any(char::is_whitespace) {
                return shortcut(command);
            }
            return Err(e.to_string());
        }
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
        Ok(())
    }
}
pub fn shortcut(path: &str) -> Result<(), String> {
    unsafe {
        let path = wide(path);
        let result = ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(path.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
        if result.0 as isize <= 32 {
            Err("shortcut launch failed".into())
        } else {
            Ok(())
        }
    }
}
