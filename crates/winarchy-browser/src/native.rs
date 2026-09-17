//! Windows-only prototype. All COM objects and filter evaluation stay on the UI STA.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::picker::{EDIT_ID, LIST_ID, Picker};
use crate::resident::{self, Exit, Request};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Instant};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use winarchy_browser::{Blocker, address, library::Library};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::*, Threading::GetCurrentThreadId},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

type AppResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wide(message).as_ptr()),
            w!("Winarchy Browser"),
            MB_OK | MB_ICONERROR,
        );
    }
}
const PALETTE: u32 = WM_APP + 1;
const HISTORY: u32 = WM_APP + 2;
const PICKER_CHANGED: u32 = WM_APP + 3;
const SUBMIT: u32 = WM_APP + 4;
const BOOKMARK: u32 = WM_APP + 5;
const LIBRARY_CHANGED: u32 = WM_APP + 6;
const THEME_CHANGED: u32 = WM_APP + 7;
#[derive(Clone)]
struct App {
    hwnd: HWND,
    picker: Rc<RefCell<Picker>>,
    brush: HBRUSH,
    surface_brush: HBRUSH,
    text: COLORREF,
    background: COLORREF,
    surface: COLORREF,
    controller: Option<ICoreWebView2Controller>,
    web: Option<ICoreWebView2>,
    home: bool,
    background_opacity: f32,
}
thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn take_string(
    f: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
) -> windows::core::Result<String> {
    let mut value = PWSTR::null();
    f(&mut value)?;
    if value.is_null() {
        return Ok(String::new());
    }
    let result = value.to_string();
    CoTaskMemFree(Some(value.0.cast()));
    Ok(result?)
}
fn color(s: &str) -> COLORREF {
    let (r, g, b) = winarchy_theme::rgb(s).unwrap();
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
fn snapshot() -> Option<App> {
    APP.with(|a| a.borrow().clone())
}
unsafe fn layout(app: &App) {
    let mut rect = RECT::default();
    let _ = GetClientRect(app.hwnd, &mut rect);
    let height = app.picker.borrow().layout(app.hwnd);
    if !app.home {
        rect.top = height.min(rect.bottom.max(0));
    }
    if let Some(c) = &app.controller {
        let _ = c.SetBounds(rect);
    }
}
unsafe fn palette(show: bool) {
    if let Some(app) = snapshot() {
        if show {
            let current = if app.home {
                String::new()
            } else {
                app.web
                    .as_ref()
                    .and_then(|w| take_string(|s| w.Source(s)).ok())
                    .unwrap_or_default()
            };
            app.picker.borrow_mut().show(app.hwnd, app.home, &current);
        } else if !app.home {
            app.picker.borrow_mut().hide();
            if let Some(c) = &app.controller {
                let _ = c.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        }
        layout(&app);
        let _ = InvalidateRect(Some(app.hwnd), None, true);
    }
}
unsafe fn refresh_theme() -> AppResult<()> {
    apply_theme(&winarchy_theme::Theme::current(
        &winarchy_theme::config_home(),
    ))
}
unsafe fn apply_theme(theme: &winarchy_theme::Theme) -> AppResult<()> {
    let new_brush = CreateSolidBrush(color(&theme.background));
    let new_surface = CreateSolidBrush(color(&theme.surface));
    let previous = APP.with(|state| {
        let mut state = state.borrow_mut();
        let app = state.as_mut().unwrap();
        let previous = (app.brush, app.surface_brush);
        app.brush = new_brush;
        app.surface_brush = new_surface;
        app.background = color(&theme.background);
        app.surface = color(&theme.surface);
        app.text = color(&theme.text);
        app.background_opacity = theme.background_opacity;
        previous
    });
    let _ = DeleteObject(previous.0.into());
    let _ = DeleteObject(previous.1.into());
    if let Some(app) = snapshot() {
        apply_opacity(&app);
        let _ = RedrawWindow(Some(app.hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
        if let Some(controller) = app
            .controller
            .and_then(|c| c.cast::<ICoreWebView2Controller2>().ok())
        {
            let (r, g, b) = winarchy_theme::rgb(&theme.background).unwrap();
            controller.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: r,
                G: g,
                B: b,
            })?;
        }
        if let Some(web) = app.web {
            web.cast::<ICoreWebView2_13>()?
                .Profile()?
                .SetPreferredColorScheme(if theme.mode.as_deref() == Some("light") {
                    COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
                } else {
                    COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
                })?;
        }
    }
    Ok(())
}
unsafe fn apply_opacity(app: &App) {
    let style = GetWindowLongPtrW(app.hwnd, GWL_EXSTYLE);
    if app.home {
        SetWindowLongPtrW(app.hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
        let alpha = (app.background_opacity * 255.0).round() as u8;
        let _ = SetLayeredWindowAttributes(app.hwnd, COLORREF(0), alpha, LWA_ALPHA);
    } else {
        // WebView2 pages must never inherit the native home's layered alpha.
        SetWindowLongPtrW(app.hwnd, GWL_EXSTYLE, style & !(WS_EX_LAYERED.0 as isize));
    }
}
unsafe fn home_mode(home: bool) {
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.home = home;
        }
    });
    if let Some(app) = snapshot() {
        apply_opacity(&app);
        if let Some(c) = &app.controller {
            let _ = c.SetIsVisible(!home);
        }
        palette(home);
    }
}
/// Route input only inside the foreground home window. Re-target key events
/// before TranslateMessage so the first character, keyboard layout and dead
/// keys are handled by the real EDIT control rather than reconstructed here.
unsafe fn route_home_input(msg: &mut MSG) {
    if !matches!(msg.message, WM_KEYDOWN | WM_KEYUP | WM_CHAR) {
        return;
    }
    let Some(app) = snapshot() else {
        return;
    };
    let edit = app.picker.borrow().edit;
    if !app.home
        || msg.hwnd == edit
        || GetForegroundWindow() != app.hwnd
        || (msg.hwnd != app.hwnd && !IsChild(app.hwnd, msg.hwnd).as_bool())
    {
        return;
    }
    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
    let alt = GetKeyState(VK_MENU.0 as i32) < 0;
    let windows = GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0;
    let editing_chord = [
        b'A' as usize,
        b'C' as usize,
        b'V' as usize,
        b'X' as usize,
        b'Y' as usize,
        b'Z' as usize,
        b'L' as usize,
        VK_HOME.0 as usize,
        VK_END.0 as usize,
        VK_LEFT.0 as usize,
        VK_RIGHT.0 as usize,
        VK_BACK.0 as usize,
        VK_DELETE.0 as usize,
    ]
    .contains(&msg.wParam.0);
    let key = matches!(msg.message, WM_KEYDOWN | WM_KEYUP)
        && !alt
        && !windows
        && (!ctrl || editing_chord);
    // Also accept queued Unicode characters (including AltGr output), but not
    // system/menu characters or control shortcuts translated to WM_CHAR.
    let character = msg.message == WM_CHAR && (msg.wParam.0 >= 32 || msg.wParam.0 == 8);
    if key || character {
        let _ = SetFocus(Some(edit));
        msg.hwnd = edit;
    }
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Keep WS_THICKFRAME for resizing/tiling, but let the page occupy the
        // entire frame instead of leaving Windows' non-client strip at the top.
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        // A layered (translucent home) window can fall back to classic frame
        // painting when deactivated, despite its client area covering the frame.
        // Keep activation bookkeeping, but suppress that non-client repaint.
        // Winarchy's separate focus ring and resize hit-testing remain intact.
        WM_NCACTIVATE if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_LAYERED.0 as isize != 0 => {
            DefWindowProcW(hwnd, msg, wp, LPARAM(-1))
        }
        WM_NCPAINT if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_LAYERED.0 as isize != 0 => {
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let hit = DefWindowProcW(hwnd, msg, wp, lp);
            if hit.0 == HTCAPTION as isize {
                LRESULT(HTCLIENT as isize)
            } else {
                hit
            }
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            if let Some(app) = snapshot() {
                FillRect(dc, &paint.rcPaint, app.brush);
            }
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(app) = snapshot() {
                layout(&app);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let rect = &*(lp.0 as *const RECT);
            if let Some(app) = snapshot() {
                app.picker.borrow_mut().set_font(hwnd);
            }
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notification = (wp.0 >> 16) & 0xffff;
            if id == EDIT_ID && notification == EN_CHANGE as usize {
                let _ = PostMessageW(Some(hwnd), PICKER_CHANGED, WPARAM(0), LPARAM(0));
            } else if id == LIST_ID && notification == LBN_DBLCLK as usize {
                let _ = PostMessageW(Some(hwnd), SUBMIT, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xffff != WA_INACTIVE as usize => {
            // Another browser window may have added a bookmark. Reload on focus,
            // not on every keystroke or on a background polling timer.
            let _ = PostMessageW(Some(hwnd), LIBRARY_CHANGED, WPARAM(0), LPARAM(0));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        LIBRARY_CHANGED => {
            if let Some(app) = snapshot()
                && app.picker.borrow().visible
            {
                let mut picker = app.picker.borrow_mut();
                if let Err(e) = picker.library.borrow_mut().reload() {
                    eprintln!("Cannot reload history: {e}");
                }
                picker.refresh(hwnd);
                drop(picker);
                layout(&app);
            }
            LRESULT(0)
        }
        PICKER_CHANGED => {
            if let Some(app) = snapshot() {
                app.picker.borrow_mut().refresh(hwnd);
                layout(&app);
            }
            LRESULT(0)
        }
        PALETTE => {
            palette(true);
            LRESULT(0)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSTATIC | WM_ERASEBKGND => APP.with(|a| {
            if let Some(a) = a.borrow().as_ref() {
                let dc = HDC(wp.0 as *mut _);
                if msg != WM_ERASEBKGND {
                    SetTextColor(dc, a.text);
                    let surface = msg != WM_CTLCOLORSTATIC;
                    SetBkColor(dc, if surface { a.surface } else { a.background });
                    return LRESULT(if surface {
                        a.surface_brush.0
                    } else {
                        a.brush.0
                    } as isize);
                }
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                FillRect(dc, &rect, a.brush);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }),
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
fn kind(context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT) -> &'static str {
    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "subdocument",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST
        | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "xmlhttprequest",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        _ => "other",
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn run(
    input: &str,
    hidden: bool,
    requests: Option<&std::sync::mpsc::Receiver<Request>>,
    ready: impl FnOnce(u32),
) -> AppResult<Exit> {
    unsafe {
        let started = Instant::now();
        let target = address(input);
        let start_home = target == "about:blank";
        let home = winarchy_theme::config_home();
        let filters = home.join("browser");
        std::fs::create_dir_all(&filters)?;
        let blocker = Rc::new(RefCell::new(Blocker::load(&filters)?));
        let library = Rc::new(RefCell::new(Library::load(&filters)?));
        eprintln!("metric filters_ready_ms={}", started.elapsed().as_millis());
        let theme = winarchy_theme::Theme::current(&home);
        let profile = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("LOCALAPPDATA is missing")?
            .join("Winarchy/browser/profile");
        std::fs::create_dir_all(&profile)?;
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("WinarchyBrowser");
        let brush = CreateSolidBrush(color(&theme.background));
        let surface_brush = CreateSolidBrush(color(&theme.surface));
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from_thread().into());
        }
        let hwnd = CreateWindowExW(
            Default::default(),
            class,
            w!("Winarchy Browser"),
            WS_POPUP
                | WS_THICKFRAME
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1100,
            760,
            None,
            None,
            Some(instance),
            None,
        )?;
        let picker = Rc::new(RefCell::new(Picker::new(hwnd, instance, library.clone())?));
        let edit = picker.borrow().edit;
        let list = picker.borrow().list;
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                picker: picker.clone(),
                brush,
                surface_brush,
                text: color(&theme.text),
                background: color(&theme.background),
                surface: color(&theme.surface),
                controller: None,
                web: None,
                home: start_home,
                background_opacity: theme.background_opacity,
            })
        });
        home_mode(start_home);
        if !hidden {
            let _ = ShowWindow(hwnd, SW_SHOW);
            if start_home {
                let _ = SetFocus(Some(edit));
            }
            eprintln!("metric window_visible_ms={}", started.elapsed().as_millis());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let profile = wide(&profile.to_string_lossy());
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR(profile.as_ptr()),
                    None,
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |result, env| {
                result?;
                let _ = tx.send(env.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                Ok(())
            }),
        )?;
        let env = rx.recv()??;
        let (tx, rx) = std::sync::mpsc::channel();
        let controller_env = env.clone();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                controller_env
                    .CreateCoreWebView2Controller(hwnd, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |result, controller| {
                result?;
                let _ = tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                Ok(())
            }),
        )?;
        let controller = rx.recv()??;
        controller.SetIsVisible(!start_home)?;
        let web = controller.CoreWebView2()?;
        if let Ok(c) = controller.cast::<ICoreWebView2Controller2>() {
            let (r, g, b) = winarchy_theme::rgb(&theme.background).unwrap();
            c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: r,
                G: g,
                B: b,
            })?;
        }
        let profile_api = web.cast::<ICoreWebView2_13>()?.Profile()?;
        profile_api.SetPreferredColorScheme(if theme.mode.as_deref() == Some("light") {
            COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
        } else {
            COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
        })?;
        let settings = web.Settings()?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(true)?;
        settings.SetIsWebMessageEnabled(false)?;
        // Keep built-in browser shortcuts (find, zoom, reload), sandbox and GPU defaults.
        let page = Rc::new(RefCell::new("about:blank".to_owned()));
        let mut token = 0;
        let page_nav = page.clone();
        web.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let uri = take_string(|s| args.Uri(s))?;
                    if !(uri.starts_with("https://")
                        || uri.starts_with("http://")
                        || uri == "about:blank")
                    {
                        args.SetCancel(true)?;
                    } else {
                        *page_nav.borrow_mut() = uri;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        // Include worker-originated requests as well as document/frame requests.
        web.cast::<ICoreWebView2_22>()?
            .AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        let request_blocker = blocker.clone();
        let request_page = page.clone();
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let request = args.Request()?;
                    let uri = take_string(|s| request.Uri(s))?;
                    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                    args.ResourceContext(&mut context)?;
                    let current = request_page.borrow();
                    // Do not block the top-level navigation itself. Referer gives a better
                    // frame source when available; referrer policy can omit or reduce it.
                    if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT && uri == *current {
                        return Ok(());
                    }
                    let source = take_string(|s| request.Headers()?.GetHeader(w!("Referer"), s))
                        .ok()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| current.clone());
                    let method = take_string(|s| request.Method(s))?;
                    if request_blocker.borrow_mut().check(
                        &uri,
                        &source,
                        kind(context),
                        &method,
                        &current,
                    ) {
                        let response = env.CreateWebResourceResponse(
                            None,
                            403,
                            w!("Blocked by Winarchy"),
                            w!("Content-Type: text/plain\r\nCache-Control: no-store"),
                        )?;
                        args.SetResponse(&response)?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        let cosmetic_blocker = blocker.clone();
        let visit_library = library.clone();
        web.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                if let Some(web) = sender {
                    let source = take_string(|s| web.Source(s))?;
                    let mut success = BOOL(0);
                    if let Some(args) = args {
                        args.IsSuccess(&mut success)?;
                    }
                    if success.as_bool() {
                        let title = take_string(|s| web.DocumentTitle(s)).unwrap_or_default();
                        if let Err(e) = visit_library.borrow_mut().visit(&source, &title) {
                            eprintln!("Cannot save history: {e}");
                        }
                    }
                    if let Some(script) = cosmetic_blocker.borrow().cosmetic_script(&source) {
                        web.ExecuteScript(PCWSTR(wide(&script).as_ptr()), None)?;
                    }
                }
                eprintln!(
                    "metric navigation_completed_ms={}",
                    started.elapsed().as_millis()
                );
                Ok(())
            })),
            &mut token,
        )?;
        // Prototype policy: target=_blank navigates this window; no hidden popup views.
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                    let uri = take_string(|s| args.Uri(s))?;
                    if let Some(web) = sender
                        && (uri.starts_with("https://") || uri.starts_with("http://"))
                    {
                        web.Navigate(PCWSTR(wide(&uri).as_ptr()))?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        // Until native permission UI exists, deny instead of silently granting access.
        web.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        let toggle_blocker = blocker.clone();
        let toggle_page = page.clone();
        controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut event = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
                    args.KeyEventKind(&mut event)?;
                    if event != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                        && event != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                    {
                        return Ok(());
                    }
                    let mut key = 0;
                    args.VirtualKey(&mut key)?;
                    if GetKeyState(VK_CONTROL.0 as i32) < 0 && key == b'L' as u32 {
                        args.SetHandled(true)?;
                        // Never manipulate focus inside the synchronous accelerator callback.
                        PostMessageW(Some(hwnd), PALETTE, WPARAM(0), LPARAM(0))?;
                    } else if GetKeyState(VK_CONTROL.0 as i32) < 0 && key == b'D' as u32 {
                        args.SetHandled(true)?;
                        let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                        args.PhysicalKeyStatus(&mut status)?;
                        if !status.WasKeyDown.as_bool() {
                            PostMessageW(Some(hwnd), BOOKMARK, WPARAM(0), LPARAM(0))?;
                        }
                    } else if GetKeyState(VK_MENU.0 as i32) < 0
                        && (key == VK_LEFT.0 as u32 || key == VK_RIGHT.0 as u32)
                    {
                        args.SetHandled(true)?;
                        PostMessageW(Some(hwnd), HISTORY, WPARAM(key as usize), LPARAM(0))?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        APP.with(|a| {
            let mut a = a.borrow_mut();
            let a = a.as_mut().unwrap();
            a.controller = Some(controller.clone());
            a.web = Some(web.clone());
        });
        if let Some(app) = snapshot() {
            layout(&app);
        }
        controller.SetIsVisible(!start_home)?;
        if !hidden {
            if start_home {
                let _ = SetFocus(Some(edit));
            } else {
                controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
            }
        }
        eprintln!("metric webview_ready_ms={}", started.elapsed().as_millis());
        if !start_home {
            web.Navigate(PCWSTR(wide(&target).as_ptr()))?;
        }
        let mut opened = !hidden;
        let mut quit = false;
        let mut warm_open_ms: Option<u128> = None;
        ready(GetCurrentThreadId());
        if requests.is_some() {
            // COM initialization pumps messages; re-signal requests queued while
            // rebuilding a spare so a consumed thread message cannot strand them.
            let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                GetCurrentThreadId(),
                resident::REQUEST,
                WPARAM(0),
                LPARAM(0),
            );
            resident::log(&format!(
                "resident_ready pid={} hidden={} ready_ms={}",
                std::process::id(),
                hidden,
                started.elapsed().as_millis()
            ));
        }
        let pending_theme = std::sync::Arc::new(std::sync::Mutex::new(None));
        let pending = pending_theme.clone();
        let window_id = hwnd.0 as usize;
        let _theme_subscription = winarchy_theme::live::watch(home.clone(), move |theme| {
            *pending.lock().unwrap() = Some(theme);
            let _ = PostMessageW(
                Some(HWND(window_id as *mut _)),
                THEME_CHANGED,
                WPARAM(0),
                LPARAM(0),
            );
        })?;
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result <= 0 {
                break;
            }
            if msg.message == THEME_CHANGED {
                if let Some(theme) = pending_theme.lock().unwrap().take()
                    && let Err(error) = apply_theme(&theme)
                {
                    eprintln!("Cannot apply theme: {error}");
                }
                continue;
            }
            if msg.message == resident::REQUEST {
                if let Some(requests) = requests {
                    while let Ok(request) = requests.try_recv() {
                        let result = if request.command == "status" {
                            Ok(serde_json::json!({"pid": std::process::id(), "ready": true, "opened": opened, "warm_open_ms": warm_open_ms}).to_string())
                        } else if request.command == "quit" {
                            quit = true;
                            if !opened {
                                let _ = DestroyWindow(hwnd);
                            }
                            Ok(if opened {
                                "stopping after window closes"
                            } else {
                                "stopping"
                            }
                            .into())
                        } else if let Some(input) = request.command.strip_prefix("open ") {
                            serde_json::from_str::<String>(input).map_err(|e| e.to_string()).and_then(|input| {
                                if opened {
                                    // Only the first window is kept warm. Do not hijack an
                                    // existing window (possibly hidden on another workspace).
                                    std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--standalone").arg(&input).spawn())
                                        .map(|child| serde_json::json!({"pid": child.id(), "warm": false}).to_string()).map_err(|e| e.to_string())
                                } else {
                                    let show_started = Instant::now();
                                    refresh_theme().map_err(|e| e.to_string())?;
                                    let target = address(&input);
                                    home_mode(target == "about:blank");
                                    opened = true;
                                    let _ = ShowWindow(hwnd, SW_SHOW);
                                    let _ = SetForegroundWindow(hwnd);
                                    if target == "about:blank" { let _ = SetFocus(Some(edit)); }
                                    else { web.Navigate(PCWSTR(wide(&target).as_ptr())).map_err(|e| e.to_string())?; }
                                    warm_open_ms = Some(show_started.elapsed().as_millis());
                                    resident::log(&format!("warm_open pid={} show_ms={}", std::process::id(), warm_open_ms.unwrap()));
                                    Ok(serde_json::json!({"pid": std::process::id(), "warm": true, "show_ms": warm_open_ms}).to_string())
                                }
                            })
                        } else {
                            Err("expected open, status or quit".into())
                        };
                        let _ = request.reply.send(result);
                    }
                }
                continue;
            }
            route_home_input(&mut msg);
            if msg.message == HISTORY {
                if msg.wParam.0 == VK_LEFT.0 as usize {
                    let _ = web.GoBack();
                } else {
                    let _ = web.GoForward();
                }
                continue;
            }
            let picker_key = (msg.hwnd == edit || msg.hwnd == list) && msg.message == WM_KEYDOWN;
            let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
            if msg.message == BOOKMARK
                || (picker_key
                    && ctrl
                    && msg.wParam.0 == b'D' as usize
                    && msg.lParam.0 & (1 << 30) == 0)
            {
                if !snapshot().is_some_and(|a| a.home) {
                    let source = take_string(|s| web.Source(s))?;
                    let title = take_string(|s| web.DocumentTitle(s)).unwrap_or_default();
                    let result = library.borrow_mut().add_bookmark(&source, &title);
                    match result {
                        Ok(Some(added)) => {
                            eprintln!("metric bookmark_added={added}");
                            if picker.borrow().visible {
                                picker.borrow_mut().refresh(hwnd);
                                picker.borrow().status(if added {
                                    "★ Favori ajouté"
                                } else {
                                    "★ Déjà dans les favoris"
                                });
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("Cannot save bookmark: {e}");
                            picker.borrow().status("Impossible d’enregistrer le favori");
                        }
                    }
                }
                continue;
            }
            if msg.message == SUBMIT || (picker_key && msg.wParam.0 == VK_RETURN.0 as usize) {
                let input = picker.borrow().input();
                if input.trim() == ":block" {
                    let result = toggle_blocker
                        .borrow_mut()
                        .toggle(&toggle_page.borrow(), &filters);
                    match result {
                        Ok(()) => {
                            web.Reload()?;
                        }
                        Err(e) => eprintln!("Cannot save blocking exception: {e}"),
                    }
                    palette(false);
                } else {
                    let target = address(&input);
                    home_mode(target == "about:blank");
                    if target != "about:blank" {
                        web.Navigate(PCWSTR(wide(&target).as_ptr()))?;
                    }
                }
                continue;
            }
            if picker_key {
                if ctrl && msg.wParam.0 == b'L' as usize {
                    palette(true);
                    continue;
                }
                if ctrl && msg.wParam.0 == b'A' as usize {
                    SendMessageW(edit, 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1)));
                    continue;
                }
                match msg.wParam.0 as u16 {
                    38 => {
                        picker.borrow().choose(-1);
                        continue;
                    }
                    40 => {
                        picker.borrow().choose(1);
                        continue;
                    }
                    27 => {
                        if snapshot().is_some_and(|a| a.home) {
                            let _ = SetWindowTextW(edit, w!(""));
                            let _ = SetFocus(Some(edit));
                        } else {
                            palette(false);
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        eprintln!("metric blocked_requests={}", blocker.borrow().blocked);
        controller.Close()?;
        if let Some(app) = APP.with(|a| a.borrow_mut().take()) {
            let _ = DeleteObject(app.brush.into());
            let _ = DeleteObject(app.surface_brush.into());
        }
        Ok(if quit { Exit::Quit } else { Exit::Closed })
    }
}
