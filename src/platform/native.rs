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
/// Expands a target rectangle so the window's visible frame lands on it.
/// Windows 10/11 windows carry invisible resize borders on three sides that
/// GetWindowRect includes but the eye does not, which skews gaps otherwise.
pub fn framed(id: isize, r: Rect) -> Rect {
    unsafe {
        let mut frame = RECT::default();
        let mut window = RECT::default();
        if DwmGetWindowAttribute(
            hwnd(id),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut frame as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            || GetWindowRect(hwnd(id), &mut window).is_err()
        {
            return r;
        }
        let border = |v: i32| v.clamp(0, 64);
        let (left, top) = (
            border(frame.left - window.left),
            border(frame.top - window.top),
        );
        let (right, bottom) = (
            border(window.right - frame.right),
            border(window.bottom - frame.bottom),
        );
        Rect {
            x: r.x - left,
            y: r.y - top,
            w: r.w + left + right,
            h: r.h + top + bottom,
        }
    }
}
/// Visible frame of a window in physical pixels, without the invisible
/// resize borders.
pub fn frame(id: isize) -> Rect {
    unsafe {
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            hwnd(id),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut r as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            return rect(id);
        }
        Rect {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        }
    }
}
/// Tints the one-pixel border the DWM draws at the frame edge so it merges
/// with the ring around it; `None` restores the system color.
pub fn dwm_border(id: isize, color: Option<&str>) {
    let value = color.map_or(DWMWA_COLOR_DEFAULT, |c| colorref(c).0);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd(id),
            DWMWA_BORDER_COLOR,
            (&value as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}
const BORDER_CLASS: &str = "WinarchyBorder";
const BORDER_COLOR: &str = "WinarchyBorderColor";
fn colorref(color: &str) -> COLORREF {
    let rgb = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0);
    COLORREF((rgb >> 16) | (rgb & 0xff00) | ((rgb & 0xff) << 16))
}
#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

/// Click-through frame window drawn just outside a client's visible frame.
/// It sits right above its client in the Z order: below it, the client's DWM
/// shadow would darken it; any higher, it would cover unrelated windows.
pub struct Border {
    id: isize,
    shape: Option<(i32, i32, i32)>,
    color: Option<u32>,
}
impl Border {
    pub fn new() -> Option<Self> {
        unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            unsafe {
                if m == WM_ERASEBKGND {
                    return LRESULT(1);
                }
                if m == WM_PAINT {
                    let key = wide(BORDER_COLOR);
                    let color = COLORREF(GetPropW(h, PCWSTR(key.as_ptr())).0 as u32);
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(h, &mut ps);
                    let brush = CreateSolidBrush(color);
                    FillRect(dc, &ps.rcPaint, brush);
                    let _ = DeleteObject(brush.into());
                    let _ = EndPaint(h, &ps);
                    return LRESULT(0);
                }
                DefWindowProcW(h, m, w, l)
            }
        }
        unsafe {
            let class = wide(BORDER_CLASS);
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            });
            let h = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            corners(h.0 as isize, true);
            Some(Self {
                id: h.0 as isize,
                shape: None,
                color: None,
            })
        }
    }
    /// Surrounds `frame` with a `width`-pixel ring of `color`, just above `client`.
    pub fn place(&mut self, client: isize, frame: Rect, width: i32, color: &str) {
        let h = hwnd(self.id);
        let (w, hgt) = (frame.w + 2 * width, frame.h + 2 * width);
        unsafe {
            let above = GetWindow(hwnd(client), GW_HWNDPREV).unwrap_or_default();
            let (insert, order) = if above == h {
                (None, SWP_NOZORDER)
            } else if above.is_invalid() {
                (Some(HWND_TOP), SET_WINDOW_POS_FLAGS(0))
            } else {
                (Some(above), SET_WINDOW_POS_FLAGS(0))
            };
            let color = colorref(color).0;
            let recolor = self.color != Some(color);
            if recolor {
                let key = wide(BORDER_COLOR);
                if SetPropW(
                    h,
                    PCWSTR(key.as_ptr()),
                    Some(HANDLE(color as usize as *mut _)),
                )
                .is_ok()
                {
                    self.color = Some(color);
                }
            }
            let shape = (w, hgt, width);
            let reshape = self.shape != Some(shape);
            if reshape {
                let outer = CreateRectRgn(0, 0, w, hgt);
                let inner = CreateRectRgn(width, width, w - width, hgt - width);
                let _ = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
                let _ = DeleteObject(inner.into());
                // Ownership transfers only on success; retry a failed update.
                if SetWindowRgn(h, Some(outer), true) != 0 {
                    self.shape = Some(shape);
                } else {
                    let _ = DeleteObject(outer.into());
                }
            }
            // Never skip visibility or stacking repair, even with cached paint.
            let _ = SetWindowPos(
                h,
                insert,
                frame.x - width,
                frame.y - width,
                w,
                hgt,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | order,
            );
            if recolor || reshape {
                let _ = InvalidateRect(Some(h), None, true);
            }
        }
    }
    pub fn hide(&self) {
        show(self.id, false);
    }
}
impl Drop for Border {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(hwnd(self.id));
        }
    }
}
/// Switches the per-user Windows color mode (apps and system surfaces) and
/// broadcasts the settings change so open applications follow.
/// Serial latest-value worker: broadcasts can wait 200ms per foreign window,
/// so they must not run on the desktop/UI thread. Rapid toggles still end in the
/// most recently requested Windows mode.
pub fn color_mode(light: bool) {
    use std::sync::{Condvar, Mutex, Once};
    static REQUEST: (Mutex<Option<bool>>, Condvar) = (Mutex::new(None), Condvar::new());
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::spawn(|| {
            loop {
                let mut pending = REQUEST.0.lock().unwrap();
                while pending.is_none() {
                    pending = REQUEST.1.wait(pending).unwrap();
                }
                let light = pending.take().unwrap();
                drop(pending);
                let started = std::time::Instant::now();
                apply_color_mode(light);
                tracing::debug!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "Windows color mode synchronized"
                );
            }
        });
    });
    *REQUEST.0.lock().unwrap() = Some(light);
    REQUEST.1.notify_one();
}
fn apply_color_mode(light: bool) {
    use windows::Win32::System::Registry::*;
    unsafe {
        let mut key = HKEY::default();
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .is_err()
        {
            return;
        }
        let wanted = u32::from(light).to_le_bytes();
        let mut changed = false;
        for name in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
            let name = wide(name);
            let mut current = [0u8; 4];
            let mut size = 4u32;
            let same = RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                None,
                Some(current.as_mut_ptr()),
                Some(&mut size),
            )
            .is_ok()
                && current == wanted;
            if !same {
                let _ = RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_DWORD, Some(&wanted));
                changed = true;
            }
        }
        let _ = RegCloseKey(key);
        if changed {
            let area = wide("ImmersiveColorSet");
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(area.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                200,
                None,
            );
        }
    }
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
/// Owning process id and executable path of a window; `None` once the
/// window is gone.
pub fn process(id: isize) -> Option<(u32, String)> {
    unsafe {
        let h = hwnd(id);
        if !IsWindow(Some(h)).as_bool() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
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
        Some((pid, exe))
    }
}
pub fn metadata(id: isize) -> Option<(String, String, bool)> {
    unsafe {
        let h = hwnd(id);
        if !IsWindow(Some(h)).as_bool() || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
        let (pid, exe) = process(id)?;
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
        // Fixed-size and always-on-top windows (Teams' compact meeting view,
        // for one) are overlays; stretching them into a tile serves nobody.
        let floating = class == "#32770"
            || GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid())
            || style & WS_THICKFRAME.0 == 0
            || ex & WS_EX_TOPMOST.0 != 0;
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
/// Repaints the bottom-right corner of every monitor, where Windows draws the
/// activation watermark. That watermark has no window of its own, so nothing
/// can cover it: only a repaint of the region clears it. Explorer provided
/// that repaint as the shell, which is why the watermark stays on screen once
/// Winarchy stops it. The drawing process could not be identified: it is
/// neither Explorer nor sihost, both of which were ruled out with a
/// DrawTextExW hook, so the region is refreshed rather than the draw blocked.
pub fn repaint_corners() {
    for monitor in monitors() {
        let band = RECT {
            left: monitor.x + monitor.w - super::dpi::scale(monitor, 420),
            top: monitor.y + monitor.h - super::dpi::scale(monitor, 180),
            right: monitor.x + monitor.w,
            bottom: monitor.y + monitor.h,
        };
        unsafe {
            let _ = RedrawWindow(
                None,
                Some(&band),
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
        }
    }
}
/// Windows 11 rounds top-level windows; tiled windows look wrong with gaps
/// between rounded corners. `square` false restores the system default.
pub fn corners(id: isize, square: bool) {
    let preference = if square {
        DWMWCP_DONOTROUND
    } else {
        DWMWCP_DEFAULT
    };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd(id),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}
pub fn show(id: isize, visible: bool) {
    unsafe {
        let _ = ShowWindow(hwnd(id), if visible { SW_SHOWNA } else { SW_HIDE });
    }
}
/// Left edge of a parked window, beside the -32000 Windows uses for minimized
/// ones. Windows clamps positions at -32768, so the width cannot be added.
const PARK_X: i32 = -32000;
pub fn parking_rect(rect: Rect) -> Rect {
    Rect { x: PARK_X, ..rect }
}
/// Parked off screen by Winarchy while its workspace is inactive. Minimized
/// windows sit at the same coordinates on their own and are left alone.
pub fn parked(id: isize) -> bool {
    rect(id).x <= PARK_X + 1000 && !minimized(id)
}
/// Off screen either way for the user: hidden, minimized or parked.
pub fn concealed(id: isize) -> bool {
    !visible(id) || minimized(id) || parked(id)
}
/// Takes a client off the desktop while its workspace is inactive. Parking
/// moves it past the left edge of any monitor (DWM cloaking is refused to
/// other processes): the window stays visible for Win32 and keeps painting,
/// so revealing it needs no repaint and the exposé can still capture it.
/// Hiding is the plain ShowWindow fallback for applications that misbehave
/// off screen.
pub fn conceal(id: isize, park: bool) {
    if park {
        let r = rect(id);
        position(id, parking_rect(r), None);
    } else {
        show(id, false);
    }
}
/// Undoes both forms of concealment, whichever mode hid the window. `home` is
/// where a parked window goes back; without it, it lands near the origin of
/// the primary monitor at its current size.
pub fn reveal(id: isize, home: Option<Rect>) {
    if parked(id) {
        let r = rect(id);
        position(
            id,
            home.unwrap_or(Rect {
                x: 64,
                y: 64,
                w: r.w,
                h: r.h,
            }),
            None,
        );
    }
    if !visible(id) {
        show(id, true);
    }
}
const SINK_CLASS: &str = "WinarchyFocusSink";
/// Creates the zero-size activatable window that takes the foreground when no
/// client can, so keystrokes never land in a window Winarchy just hid. It must
/// live in a process without the keyboard hook: Windows does not run a
/// low-level hook for input aimed at the hooking process's own windows.
pub fn create_sink() -> Result<isize, String> {
    unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // Alt chords leave a bare Alt press/release here; DefWindowProc would
        // enter menu mode on it.
        let menu = m == WM_SYSCOMMAND && (w.0 & 0xfff0) as u32 == SC_KEYMENU;
        if menu || m == WM_SYSKEYDOWN || m == WM_SYSKEYUP || m == WM_SYSCHAR {
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(h, m, w, l) }
    }
    unsafe {
        let class = wide(SINK_CLASS);
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .map_err(|e| e.to_string())?;
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        });
        let h = CreateWindowExW(
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
        )
        .map_err(|e| e.to_string())?;
        Ok(h.0 as isize)
    }
}
/// The sink window owned by `pid`, or 0 when it is not available.
pub fn sink(pid: u32) -> isize {
    if pid == 0 {
        return 0;
    }
    let class = wide(SINK_CLASS);
    unsafe {
        let mut after = HWND::default();
        while let Ok(h) = FindWindowExW(None, Some(after), PCWSTR(class.as_ptr()), None) {
            let mut owner = 0;
            GetWindowThreadProcessId(h, Some(&mut owner));
            if owner == pid {
                return h.0 as isize;
            }
            after = h;
        }
    }
    0
}
/// `warp` centers the pointer on the window so the mouse follows keyboard-driven
/// focus; pointer-driven focus passes `false` to leave the cursor alone.
pub fn focus(id: isize, warp: bool) {
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
        // process the last input source, which is one of the allowed cases. The
        // key must not be a modifier: the user is usually still holding Alt, and
        // a released Alt also makes Win32 applications show their menu bar.
        if !SetForegroundWindow(hwnd(id)).as_bool() {
            tracing::debug!(id, "foreground fallback");
            let release = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_NONAME,
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
        if warp {
            let r = rect(id);
            let _ = SetCursorPos(r.x + r.w / 2, r.y + r.h / 2);
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
    if items.is_empty() {
        return;
    }
    let fallback = || {
        for (id, rect) in items {
            position(*id, *rect, None);
        }
    };
    unsafe {
        let Ok(mut d) = BeginDeferWindowPos(items.len() as i32) else {
            fallback();
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
                    tracing::debug!(%e,"defer failed; applying placements individually");
                    fallback();
                    return;
                }
            }
        }
        if let Err(error) = EndDeferWindowPos(d) {
            tracing::debug!(%error, "batch failed; applying placements individually");
            fallback();
        }
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
/// Packaged (MSIX/Store) applications from the Applications shell folder,
/// as (display name, `shell:AppsFolder\AUMID`) launch targets. They have no
/// Start Menu .lnk files, so the shortcut scan never sees them.
pub fn packaged_apps() -> Vec<(String, String)> {
    use windows::Win32::{System::Com::*, UI::Shell::*};
    let mut out = Vec::new();
    unsafe {
        // This enumeration runs on a dedicated worker. Balance even S_FALSE;
        // the guard is declared before interfaces so they are released first.
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            return out;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let Ok(folder) =
            SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
        else {
            return out;
        };
        let Ok(items) =
            folder.BindToHandler::<Option<&IBindCtx>, IEnumShellItems>(None, &BHID_EnumItems)
        else {
            return out;
        };
        let text = |name: windows::core::PWSTR| {
            let value = name.to_string().unwrap_or_default();
            CoTaskMemFree(Some(name.0.cast()));
            value
        };
        while out.len() < 8192 {
            let mut batch = [None];
            let mut fetched = 0;
            if items.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = batch[0].take() else { break };
            let (Ok(name), Ok(parsing)) = (
                item.GetDisplayName(SIGDN_NORMALDISPLAY).map(text),
                item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).map(text),
            ) else {
                continue;
            };
            // Unpackaged entries duplicate the .lnk scan; packaged ones carry an AUMID.
            if parsing.contains('!') && !name.is_empty() {
                out.push((name, format!("shell:AppsFolder\\{parsing}")));
            }
        }
    }
    out
}
pub fn shortcut(path: &str) -> Result<(), String> {
    // ShellExecute on shell:AppsFolder targets is refused without Explorer;
    // the activation manager launches packaged applications directly.
    if let Some(aumid) = path.strip_prefix("shell:AppsFolder\\") {
        use windows::Win32::{System::Com::*, UI::Shell::*};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                    .map_err(|e| e.to_string())?;
            let id = wide(aumid);
            manager
                .ActivateApplication(PCWSTR(id.as_ptr()), PCWSTR::null(), AO_NONE)
                .map(|_| ())
                .map_err(|e| format!("application activation failed: {e}"))
        }
    } else {
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
}
