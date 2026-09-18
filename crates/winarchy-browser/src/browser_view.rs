//! One isolated WebView controller per tab; all tabs share one environment/profile.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::native::{self, wide};
use std::{cell::RefCell, rc::Rc, time::Instant};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use winarchy_browser::{
    Blocker,
    library::Library,
    tabs::{TabId, Tabs},
};
use windows::{
    Win32::{Foundation::*, System::Com::CoTaskMemFree},
    core::*,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct BrowserView {
    pub controller: ICoreWebView2Controller,
    pub web: ICoreWebView2,
}
impl Drop for BrowserView {
    fn drop(&mut self) {
        unsafe {
            let _ = self.controller.Close();
        }
    }
}
#[derive(Clone)]
pub struct ViewContext {
    pub hwnd: HWND,
    pub env: ICoreWebView2Environment,
    pub blocker: Rc<RefCell<Blocker>>,
    pub library: Rc<RefCell<Library>>,
    pub tabs: Rc<RefCell<Tabs>>,
    pub started: Instant,
}
unsafe fn string(
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
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        _ => "other",
    }
}
impl ViewContext {
    /// Called only by the outer host loop, never from a synchronous WebView
    /// callback. The library pumps initialization messages without holding any
    /// tab/picker borrow. New views stay hidden until the host activates them.
    pub unsafe fn create(&self, id: TabId) -> Result<Rc<BrowserView>> {
        let env = self.env.clone();
        let hwnd = self.hwnd;
        let (tx, rx) = std::sync::mpsc::channel();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                env.CreateCoreWebView2Controller(hwnd, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |result, controller| {
                result?;
                let _ = tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                Ok(())
            }),
        )?;
        let controller = rx.recv()??;
        let web = match controller.CoreWebView2() {
            Ok(web) => web,
            Err(error) => {
                let _ = controller.Close();
                return Err(error.into());
            }
        };
        let view = Rc::new(BrowserView { controller, web });
        view.controller.SetIsVisible(false)?;
        let web = &view.web;
        let theme = winarchy_theme::Theme::current(&winarchy_theme::config_home());
        let (r, g, b) = winarchy_theme::rgb(&theme.background).unwrap();
        view.controller
            .cast::<ICoreWebView2Controller2>()?
            .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: r,
                G: g,
                B: b,
            })?;
        web.cast::<ICoreWebView2_13>()?
            .Profile()?
            .SetPreferredColorScheme(if theme.mode.as_deref() == Some("light") {
                COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
            } else {
                COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
            })?;
        let settings = web.Settings()?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(true)?;
        settings.SetIsWebMessageEnabled(false)?;
        let page = Rc::new(RefCell::new("about:blank".to_owned()));
        let mut token = 0;
        let page_nav = page.clone();
        web.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let uri = string(|s| args.Uri(s))?;
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
        let tabs = self.tabs.clone();
        web.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web) = sender {
                    let source = string(|s| web.Source(s))?;
                    if let Some(tab) = tabs.borrow_mut().get_mut(id) {
                        tab.home = source == "about:blank";
                        tab.url = source;
                    }
                    native::tabs_changed(hwnd);
                }
                Ok(())
            })),
            &mut token,
        )?;
        let tabs = self.tabs.clone();
        web.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web) = sender {
                    let title = string(|s| web.DocumentTitle(s))?;
                    if let Some(tab) = tabs.borrow_mut().get_mut(id) {
                        tab.title = title
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(200)
                            .collect();
                    }
                    native::tabs_changed(hwnd);
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.cast::<ICoreWebView2_22>()?
            .AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        let request_blocker = self.blocker.clone();
        let request_page = page;
        let env = self.env.clone();
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let request = args.Request()?;
                    let uri = string(|s| request.Uri(s))?;
                    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                    args.ResourceContext(&mut context)?;
                    let current = request_page.borrow().clone();
                    if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT && uri == current {
                        return Ok(());
                    }
                    let source = string(|s| request.Headers()?.GetHeader(w!("Referer"), s))
                        .ok()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| current.clone());
                    let method = string(|s| request.Method(s))?;
                    let blocked = request_blocker.borrow_mut().check(
                        &uri,
                        &source,
                        kind(context),
                        &method,
                        &current,
                    );
                    if blocked {
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
        let cosmetic_blocker = self.blocker.clone();
        let library = self.library.clone();
        let started = self.started;
        web.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                if let Some(web) = sender {
                    let source = string(|s| web.Source(s))?;
                    let mut success = BOOL(0);
                    if let Some(args) = args {
                        args.IsSuccess(&mut success)?;
                    }
                    if success.as_bool() {
                        let title = string(|s| web.DocumentTitle(s)).unwrap_or_default();
                        if let Err(error) = library.borrow_mut().visit(&source, &title) {
                            eprintln!("Cannot save history: {error}");
                        }
                    }
                    let script = cosmetic_blocker.borrow().cosmetic_script(&source);
                    if let Some(script) = script {
                        web.ExecuteScript(PCWSTR(wide(&script).as_ptr()), None)?;
                    }
                }
                native::tabs_changed(hwnd);
                eprintln!(
                    "metric navigation_completed_ms={}",
                    started.elapsed().as_millis()
                );
                Ok(())
            })),
            &mut token,
        )?;
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                    let uri = string(|s| args.Uri(s))?;
                    if uri.starts_with("https://") || uri.starts_with("http://") {
                        native::queue_popup(id, uri);
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        let audio = web.cast::<ICoreWebView2_8>()?;
        let tabs = self.tabs.clone();
        audio.add_IsDocumentPlayingAudioChanged(
            &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web) = sender {
                    let mut playing = BOOL(0);
                    web.cast::<ICoreWebView2_8>()?
                        .IsDocumentPlayingAudio(&mut playing)?;
                    if let Some(tab) = tabs.borrow_mut().get_mut(id) {
                        tab.audible = playing.as_bool();
                    }
                    native::tabs_changed(hwnd);
                }
                Ok(())
            })),
            &mut token,
        )?;
        let tabs = self.tabs.clone();
        audio.add_IsMutedChanged(
            &IsMutedChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(web) = sender {
                    let mut muted = BOOL(0);
                    web.cast::<ICoreWebView2_8>()?.IsMuted(&mut muted)?;
                    if let Some(tab) = tabs.borrow_mut().get_mut(id) {
                        tab.muted = muted.as_bool();
                    }
                    native::tabs_changed(hwnd);
                }
                Ok(())
            })),
            &mut token,
        )?;
        view.controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                native::tab_got_focus(id, hwnd);
                Ok(())
            })),
            &mut token,
        )?;
        view.controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    native::web_accelerator(id, &args)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        Ok(view)
    }
}
