//! Windows-only prototype. All COM objects and filter evaluation stay on the UI STA.
#![allow(unsafe_op_in_unsafe_fn)]
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Instant};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use winarchy_browser::{Blocker, address};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::*},
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
#[derive(Clone)]
struct App {
    hwnd: HWND,
    edit: HWND,
    brush: HBRUSH,
    text: COLORREF,
    background: COLORREF,
    controller: Option<ICoreWebView2Controller>,
    web: Option<ICoreWebView2>,
    palette: bool,
}
thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }
fn wide(s: &str) -> Vec<u16> {
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
unsafe fn layout(app: &App) {
    let mut rect = RECT::default();
    let _ = GetClientRect(app.hwnd, &mut rect);
    if app.palette {
        let _ = SetWindowPos(
            app.edit,
            Some(HWND_TOP),
            8,
            8,
            (rect.right - 16).max(1),
            28,
            SWP_NOACTIVATE,
        );
        rect.top = 44;
    }
    if let Some(c) = &app.controller {
        let _ = c.SetBounds(rect);
    }
}
unsafe fn palette(show: bool) {
    let snapshot = APP.with(|cell| {
        let mut state = cell.borrow_mut();
        let app = state.as_mut()?;
        app.palette = show;
        Some(app.clone())
    });
    if let Some(app) = snapshot.as_ref() {
        if show {
            if let Some(web) = &app.web
                && let Ok(url) = take_string(|s| web.Source(s))
            {
                let _ = SetWindowTextW(app.edit, PCWSTR(wide(&url).as_ptr()));
            }
            let _ = ShowWindow(app.edit, SW_SHOW);
            layout(app);
            let _ = SetFocus(Some(app.edit));
            SendMessageW(
                app.edit,
                0x00B1, /* EM_SETSEL */
                Some(WPARAM(0)),
                Some(LPARAM(-1)),
            );
        } else {
            let _ = ShowWindow(app.edit, SW_HIDE);
            layout(app);
            if let Some(c) = &app.controller {
                let _ = c.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            }
        }
    }
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            APP.with(|a| {
                if let Some(a) = a.borrow().as_ref() {
                    layout(a);
                }
            });
            LRESULT(0)
        }
        PALETTE => {
            palette(true);
            LRESULT(0)
        }
        WM_CTLCOLOREDIT | WM_ERASEBKGND => APP.with(|a| {
            if let Some(a) = a.borrow().as_ref() {
                let dc = HDC(wp.0 as *mut _);
                if msg == WM_CTLCOLOREDIT {
                    SetTextColor(dc, a.text);
                    SetBkColor(dc, a.background);
                    return LRESULT(a.brush.0 as isize);
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

pub fn run() -> AppResult<()> {
    unsafe {
        let started = Instant::now();
        let home = winarchy_theme::config_home();
        let filters = home.join("browser");
        std::fs::create_dir_all(&filters)?;
        let blocker = Rc::new(RefCell::new(Blocker::load(&filters)?));
        eprintln!("metric filters_ready_ms={}", started.elapsed().as_millis());
        let theme = winarchy_theme::Theme::current(&home);
        let profile = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("LOCALAPPDATA is missing")?
            .join("Winarchy/browser/profile");
        std::fs::create_dir_all(&profile)?;
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("WinarchyBrowser");
        let brush = CreateSolidBrush(color(&theme.background));
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_thread().into());
        }
        let hwnd = CreateWindowExW(
            Default::default(),
            class,
            w!("Winarchy Browser"),
            WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1100,
            760,
            None,
            None,
            Some(instance),
            None,
        )?;
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            8,
            8,
            1084,
            28,
            Some(hwnd),
            None,
            Some(instance),
            None,
        )?;
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                edit,
                brush,
                text: color(&theme.text),
                background: color(&theme.background),
                controller: None,
                web: None,
                palette: false,
            })
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        eprintln!("metric window_visible_ms={}", started.elapsed().as_millis());
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
        web.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web) = sender {
                    let source = take_string(|s| web.Source(s))?;
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
            layout(a);
        });
        controller.SetIsVisible(true)?;
        controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
        eprintln!("metric webview_ready_ms={}", started.elapsed().as_millis());
        let target = address(&std::env::args().skip(1).collect::<Vec<_>>().join(" "));
        web.Navigate(PCWSTR(wide(&target).as_ptr()))?;
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result <= 0 {
                break;
            }
            if msg.message == HISTORY {
                if msg.wParam.0 == VK_LEFT.0 as usize {
                    let _ = web.GoBack();
                } else {
                    let _ = web.GoForward();
                }
                continue;
            }
            if msg.hwnd == edit && msg.message == WM_KEYDOWN {
                match msg.wParam.0 as u16 {
                    13 => {
                        let mut text = vec![0u16; GetWindowTextLengthW(edit) as usize + 1];
                        GetWindowTextW(edit, &mut text);
                        let input = String::from_utf16_lossy(&text[..text.len() - 1]);
                        if input.trim() == ":block" {
                            let result = toggle_blocker
                                .borrow_mut()
                                .toggle(&toggle_page.borrow(), &filters);
                            match result {
                                Ok(()) => {
                                    eprintln!(
                                        "blocking_enabled={}",
                                        toggle_blocker.borrow().enabled(&toggle_page.borrow())
                                    );
                                    web.Reload()?;
                                }
                                Err(e) => eprintln!("Cannot save blocking exception: {e}"),
                            }
                        } else {
                            web.Navigate(PCWSTR(wide(&address(&input)).as_ptr()))?;
                        }
                        palette(false);
                        continue;
                    }
                    27 => {
                        palette(false);
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
        APP.with(|a| {
            a.borrow_mut().take();
        });
        let _ = DeleteObject(brush.into());
        // COM interfaces are dropped before process exit; no resident helper is kept.
        Ok(())
    }
}
