//! WebView2 popups for applets whose manifest names a `web` page. Each applet
//! owns a hidden tool window holding one controller, created at load and kept
//! until the applet changes; all share one environment and profile.
use super::super::{Event, EventSender, native};
use crate::{
    applets::{Applet, web},
    config::Theme,
    layout::Rect,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
#[cfg(test)]
mod tests;
use webview2_com::{
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DownloadStartingEventHandler,
    Microsoft::Web::WebView2::Win32::*, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, ProcessFailedEventHandler,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};
use windows62::{
    Win32::{Foundation::RECT, System::Com::CoTaskMemFree},
    core::{HSTRING, Interface, PCWSTR, PWSTR},
};

type Shared = Rc<RefCell<State>>;
enum Environment {
    Idle,
    Creating(Vec<Weak<RefCell<State>>>),
    Ready(ICoreWebView2Environment),
}
thread_local! {
    static ENVIRONMENT: RefCell<Environment> = const { RefCell::new(Environment::Idle) };
}
struct State {
    name: String,
    generation: u64,
    host: String,
    url: String,
    /// A page of the applet folder, served and scripted by Illium; a remote
    /// `site` is only shown.
    local: bool,
    dir: std::path::PathBuf,
    window: isize,
    tx: EventSender,
    controller: Option<ICoreWebView2Controller>,
    web: Option<ICoreWebView2>,
    /// Creation failed; reported on the next opening, which retries.
    failure: Option<String>,
    /// The browser process exited; the next opening starts over.
    lost: bool,
    /// The injected script of the current document is listening.
    ready: bool,
    theme: Theme,
    data: Value,
    busy: bool,
    error: String,
    open: bool,
    focus: bool,
    size: (i32, i32),
    zoom: Option<f64>,
}
pub struct View(Shared);
impl Drop for View {
    fn drop(&mut self) {
        let (controller, window) = {
            let mut state = self.0.borrow_mut();
            (state.controller.take(), state.window)
        };
        unsafe {
            if let Some(controller) = controller {
                let _ = controller.Close();
            }
            let _ = DestroyWindow(native::hwnd(window));
        }
    }
}
impl View {
    pub fn new(
        applet: &Applet,
        generation: u64,
        theme: &Theme,
        tx: &EventSender,
    ) -> Result<Self, String> {
        let (host, url, local) = match (&applet.manifest.site, &applet.manifest.web) {
            (Some(site), _) => {
                let host = web::site_host(site).ok_or("site is not a plain https:// URL")?;
                (host, site.clone(), false)
            }
            (None, Some(page)) => (web::host(&applet.name), web::url(&applet.name, page), true),
            (None, None) => return Err("neither web nor site in the manifest".into()),
        };
        let window = create_window(applet.manifest.focusable)?;
        let view = Self(Rc::new(RefCell::new(State {
            name: applet.name.clone(),
            generation,
            host,
            url,
            local,
            dir: applet.dir.clone(),
            window,
            tx: tx.clone(),
            controller: None,
            web: None,
            failure: None,
            lost: false,
            ready: false,
            theme: theme.clone(),
            data: Value::Null,
            busy: false,
            error: String::new(),
            open: false,
            focus: false,
            size: (1, 1),
            zoom: applet.manifest.zoom,
        })));
        start(&view.0);
        Ok(view)
    }
    pub fn window(&self) -> isize {
        self.0.borrow().window
    }
    /// The last creation error, once; the view starts over so a later opening
    /// can succeed, for example after the WebView2 runtime was installed.
    pub fn take_failure(&self) -> Option<String> {
        let failure = self.0.borrow_mut().failure.take()?;
        start(&self.0);
        Some(failure)
    }
    pub fn theme(&self, theme: &Theme) {
        self.0.borrow_mut().theme = theme.clone();
        let (controller, web) = {
            let state = self.0.borrow();
            (state.controller.clone(), state.web.clone())
        };
        if let (Some(controller), Some(web)) = (controller, web) {
            let _ = unsafe { colors(&controller, &web, theme) };
        }
        post(&self.0, "theme", web::theme(theme));
    }
    pub fn data(&self, data: &Value) {
        self.0.borrow_mut().data = data.clone();
        post(&self.0, "data", data.clone());
    }
    pub fn busy(&self, busy: bool) {
        self.0.borrow_mut().busy = busy;
        post(&self.0, "busy", json!(busy));
    }
    pub fn error(&self, error: &str) {
        self.0.borrow_mut().error = error.to_owned();
        post(&self.0, "error", json!(error));
    }
    pub fn reload(&self) {
        let web = self.0.borrow().web.clone();
        if let Some(web) = web
            && let Err(error) = unsafe { web.Reload() }
        {
            tracing::warn!(applet = self.0.borrow().name, %error, "web applet reload failed");
        }
    }
    pub fn completed(&self) {
        post(&self.0, "completed", Value::Null);
    }
    pub fn show(&self, r: Rect, focus: bool) {
        let (window, controller, lost) = {
            let mut state = self.0.borrow_mut();
            state.open = true;
            state.focus = focus;
            state.size = (r.w, r.h);
            let lost = std::mem::take(&mut state.lost);
            (state.window, state.controller.clone(), lost)
        };
        if lost {
            start(&self.0);
        }
        native::position(window, r, Some(HWND_TOPMOST));
        unsafe {
            let _ = ShowWindow(native::hwnd(window), SW_SHOWNOACTIVATE);
        }
        if focus {
            native::focus(window, false);
        }
        if let Some(controller) = controller {
            let _ = unsafe { reveal(&controller, (r.w, r.h), focus) };
        }
        post(&self.0, "open", Value::Null);
    }
    pub fn hide(&self) {
        post(&self.0, "dismissed", Value::Null);
        let (window, controller) = {
            let mut state = self.0.borrow_mut();
            state.open = false;
            (state.window, state.controller.clone())
        };
        unsafe {
            if let Some(controller) = controller {
                let _ = controller.SetIsVisible(false);
            }
            let _ = ShowWindow(native::hwnd(window), SW_HIDE);
        }
    }
}
unsafe extern "system" fn window_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Alt+F4 would destroy the window under the runtime; Escape closes popups.
    if m == WM_CLOSE {
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}
fn create_window(focusable: bool) -> Result<isize, String> {
    use windows::core::w;
    static CLASS: std::sync::Once = std::sync::Once::new();
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        CLASS.call_once(|| {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("IlliumWebApplet"),
                ..Default::default()
            });
        });
        let mut style = WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        if !focusable {
            style |= WS_EX_NOACTIVATE;
        }
        let window = CreateWindowExW(
            style,
            w!("IlliumWebApplet"),
            w!("Illium applet"),
            WS_POPUP | WS_CLIPCHILDREN,
            -32000,
            -32000,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("cannot create the web view window: {e}"))?;
        let id = window.0 as isize;
        native::corners(id, true);
        Ok(id)
    }
}
fn hwnd62(id: isize) -> windows62::Win32::Foundation::HWND {
    windows62::Win32::Foundation::HWND(id as *mut _)
}
unsafe fn string(
    f: impl FnOnce(*mut PWSTR) -> windows62::core::Result<()>,
) -> windows62::core::Result<String> {
    let mut value = PWSTR::null();
    f(&mut value)?;
    if value.is_null() {
        return Ok(String::new());
    }
    let result = unsafe { value.to_string() };
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    Ok(result?)
}
/// Sends one message to the page once its injected script listens; the
/// script's `ready` request replays the sticky state for later documents.
fn post(state: &Shared, kind: &str, value: Value) {
    let web = {
        let state = state.borrow();
        if !state.ready {
            return;
        }
        state.web.clone()
    };
    if let Some(web) = web {
        let message = HSTRING::from(web::message(kind, value));
        if let Err(error) = unsafe { web.PostWebMessageAsJson(&message) } {
            tracing::debug!(%error, "web applet message not delivered");
        }
    }
}
fn replay(state: &Shared) {
    let (theme, data, busy, error, open) = {
        let s = state.borrow();
        (
            web::theme(&s.theme),
            s.data.clone(),
            s.busy,
            s.error.clone(),
            s.open,
        )
    };
    post(state, "theme", theme);
    if !data.is_null() {
        post(state, "data", data);
    }
    post(state, "busy", json!(busy));
    post(state, "error", json!(error));
    if open {
        post(state, "open", Value::Null);
    }
}
fn fail(state: &Shared, error: String) {
    tracing::warn!(applet = state.borrow().name, %error, "web view unavailable");
    state.borrow_mut().failure = Some(error);
}
/// Requests a controller, creating the shared environment first if needed.
fn start(state: &Shared) {
    enum Next {
        Use(ICoreWebView2Environment),
        Wait,
        Create,
    }
    let weak = Rc::downgrade(state);
    let next = ENVIRONMENT.with(|slot| {
        let mut slot = slot.borrow_mut();
        match &mut *slot {
            Environment::Ready(env) => Next::Use(env.clone()),
            Environment::Creating(waiting) => {
                waiting.push(weak.clone());
                Next::Wait
            }
            Environment::Idle => {
                *slot = Environment::Creating(vec![weak.clone()]);
                Next::Create
            }
        }
    });
    match next {
        Next::Use(env) => controller(&env, weak),
        Next::Wait => {}
        Next::Create => {
            if let Err(error) = environment() {
                finish_environment(Err(error));
            }
        }
    }
}
fn environment() -> Result<(), String> {
    let mut version = PWSTR::null();
    unsafe {
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version)
            .map_err(|e| format!("Microsoft Edge WebView2 Runtime not found: {e}"))?;
        if version.is_null() {
            return Err("Microsoft Edge WebView2 Runtime not found".into());
        }
        CoTaskMemFree(Some(version.0.cast()));
    }
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let folder = std::path::Path::new(&local).join("Illium/applet-webview");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let folder = HSTRING::from(folder.as_os_str());
    let options: ICoreWebView2EnvironmentOptions = CoreWebView2EnvironmentOptions::default().into();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        finish_environment(
            result
                .and_then(|()| env.ok_or_else(|| windows62::Win32::Foundation::E_POINTER.into()))
                .map_err(|e| format!("WebView2 environment failed: {e}")),
        );
        Ok(())
    }));
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &folder, &options, &handler)
            .map_err(|e| format!("WebView2 environment failed: {e}"))
    }
}
fn finish_environment(result: Result<ICoreWebView2Environment, String>) {
    let waiting = ENVIRONMENT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let waiting = match std::mem::replace(&mut *slot, Environment::Idle) {
            Environment::Creating(waiting) => waiting,
            _ => vec![],
        };
        if let Ok(env) = &result {
            *slot = Environment::Ready(env.clone());
        }
        waiting
    });
    for weak in waiting {
        match &result {
            Ok(env) => controller(env, weak),
            Err(error) => {
                if let Some(state) = weak.upgrade() {
                    fail(&state, error.clone());
                }
            }
        }
    }
}
fn controller(env: &ICoreWebView2Environment, weak: Weak<RefCell<State>>) {
    let Some(state) = weak.upgrade() else { return };
    let window = state.borrow().window;
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |result, controller| {
            let Some(state) = weak.upgrade() else {
                if let Some(controller) = controller {
                    unsafe {
                        let _ = controller.Close();
                    }
                }
                return Ok(());
            };
            let result = result
                .and_then(|()| {
                    controller.ok_or_else(|| windows62::Win32::Foundation::E_POINTER.into())
                })
                .and_then(|controller| unsafe { setup(&state, controller) });
            if let Err(error) = result {
                fail(&state, format!("WebView2 controller failed: {error}"));
            }
            Ok(())
        },
    ));
    if let Err(error) = unsafe { env.CreateCoreWebView2Controller(hwnd62(window), &handler) } {
        fail(&state, format!("WebView2 controller failed: {error}"));
    }
}
unsafe fn colors(
    controller: &ICoreWebView2Controller,
    web: &ICoreWebView2,
    theme: &Theme,
) -> windows62::core::Result<()> {
    let (r, g, b) = illium_theme::rgb(&theme.background).unwrap_or((0, 0, 0));
    unsafe {
        controller
            .cast::<ICoreWebView2Controller2>()?
            .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: r,
                G: g,
                B: b,
            })?;
        web.cast::<ICoreWebView2_13>()?
            .Profile()?
            .SetPreferredColorScheme(if web::dark(theme) {
                COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
            } else {
                COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
            })
    }
}
unsafe fn reveal(
    controller: &ICoreWebView2Controller,
    (w, h): (i32, i32),
    focus: bool,
) -> windows62::core::Result<()> {
    unsafe {
        controller.SetBounds(RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        })?;
        controller.SetIsVisible(true)?;
        if focus {
            controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
        }
    }
    Ok(())
}
fn on_message(state: &Shared, source: &str, json: &str) {
    let (host, name, generation, tx, open) = {
        let s = state.borrow();
        (
            s.host.clone(),
            s.name.clone(),
            s.generation,
            s.tx.clone(),
            s.open,
        )
    };
    if !web::allowed(&host, source, true) {
        return;
    }
    match web::request(json) {
        Ok(web::Request::Ready) => {
            state.borrow_mut().ready = true;
            replay(state);
        }
        Ok(web::Request::Action(action)) => {
            let _ = tx.send(Event::AppletAction(name, generation, action));
        }
        // A hidden page must not close another applet's popup.
        Ok(web::Request::Close) if open => {
            let _ = tx.send(Event::Escape);
        }
        Ok(web::Request::Close) => {}
        Err(error) => tracing::warn!(applet = name, %error, "web applet message ignored"),
    }
}
/// Locks the page to its folder and wires it to the state, then navigates.
unsafe fn setup(
    state: &Shared,
    controller: ICoreWebView2Controller,
) -> windows62::core::Result<()> {
    let web = unsafe { controller.CoreWebView2()? };
    let (host, url, local, dir, theme, open, size, focus, zoom, env) = {
        let s = state.borrow();
        (
            s.host.clone(),
            s.url.clone(),
            s.local,
            s.dir.clone(),
            s.theme.clone(),
            s.open,
            s.size,
            s.focus,
            s.zoom,
            ENVIRONMENT.with(|slot| match &*slot.borrow() {
                Environment::Ready(env) => Some(env.clone()),
                _ => None,
            }),
        )
    };
    let env =
        env.ok_or_else(|| windows62::core::Error::from(windows62::Win32::Foundation::E_POINTER))?;
    unsafe {
        controller.SetIsVisible(false)?;
        if let Some(zoom) = zoom {
            controller.SetZoomFactor(zoom)?;
        }
        colors(&controller, &web, &theme)?;
        let settings = web.Settings()?;
        settings.SetAreDevToolsEnabled(false)?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetIsZoomControlEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetIsWebMessageEnabled(true)?;
        settings
            .cast::<ICoreWebView2Settings3>()?
            .SetAreBrowserAcceleratorKeysEnabled(false)?;
        let settings = settings.cast::<ICoreWebView2Settings5>()?;
        settings.SetIsPinchZoomEnabled(false)?;
        settings.SetIsGeneralAutofillEnabled(false)?;
        settings.SetIsPasswordAutosaveEnabled(false)?;
        if local {
            web.cast::<ICoreWebView2_3>()?
                .SetVirtualHostNameToFolderMapping(
                    &HSTRING::from(host.as_str()),
                    &HSTRING::from(dir.as_os_str()),
                    COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY,
                )?;
        }
        let mut token = 0;
        let weak = Rc::downgrade(state);
        let navigation_host = host.clone();
        web.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let uri = string(|s| args.Uri(s))?;
                    if !web::allowed(&navigation_host, &uri, true) {
                        tracing::warn!(%uri, "web applet navigation blocked");
                        args.SetCancel(true)?;
                    } else if let Some(state) = weak.upgrade() {
                        state.borrow_mut().ready = false;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        let frame_host = host.clone();
        web.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let uri = string(|s| args.Uri(s))?;
                    if !web::allowed(&frame_host, &uri, true) {
                        args.SetCancel(true)?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.cast::<ICoreWebView2_22>()?
            .AddWebResourceRequestedFilterWithRequestSourceKinds(
                windows62::core::w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        let request_host = host.clone();
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let uri = string(|s| args.Request()?.Uri(s))?;
                    if !web::allowed(&request_host, &uri, false) {
                        tracing::debug!(%uri, "web applet request blocked");
                        let response = env.CreateWebResourceResponse(
                            None,
                            403,
                            windows62::core::w!("Blocked by Illium"),
                            windows62::core::w!(
                                "Content-Type: text/plain\r\nCache-Control: no-store"
                            ),
                        )?;
                        args.SetResponse(&response)?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.cast::<ICoreWebView2_4>()?.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        let weak = Rc::downgrade(state);
        web.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                if let (Some(args), Some(state)) = (args, weak.upgrade()) {
                    let source = string(|s| args.Source(s))?;
                    let json = string(|s| args.WebMessageAsJson(s))?;
                    on_message(&state, &source, &json);
                }
                Ok(())
            })),
            &mut token,
        )?;
        let weak = Rc::downgrade(state);
        web.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |sender, args| {
                let (Some(args), Some(state)) = (args, weak.upgrade()) else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED;
                args.ProcessFailedKind(&mut kind)?;
                tracing::warn!(
                    applet = state.borrow().name,
                    kind = kind.0,
                    "web view process failed"
                );
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    // Every controller died with the browser process: the next
                    // opening of each view starts over with a new environment.
                    ENVIRONMENT.with(|slot| *slot.borrow_mut() = Environment::Idle);
                    let controller = {
                        let mut s = state.borrow_mut();
                        s.ready = false;
                        s.web = None;
                        s.lost = true;
                        s.controller.take()
                    };
                    if let Some(controller) = controller {
                        let _ = controller.Close();
                    }
                } else if (kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                    || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE)
                    && let Some(web) = sender
                {
                    web.Reload()?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        if local {
            web.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(web::SCRIPT), None)?;
        }
        web.Navigate(&HSTRING::from(url.as_str()))?;
        {
            let mut s = state.borrow_mut();
            s.controller = Some(controller.clone());
            s.web = Some(web);
        }
        if open {
            reveal(&controller, size, focus)?;
        }
    }
    Ok(())
}
