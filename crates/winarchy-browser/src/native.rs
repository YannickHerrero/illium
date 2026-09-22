//! Windows-only prototype. All COM objects and filter evaluation stay on the UI STA.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::browser_view::{BrowserView, ViewContext};
use crate::leader_panel::LeaderPanel;
use crate::picker::{EDIT_ID, LIST_ID, Picker};
use crate::resident::{self, Exit, Request};
use std::collections::{HashMap, HashSet, VecDeque};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Instant};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use winarchy_browser::leader::{Action, CapturedKeys, Key, Leader, Outcome};
use winarchy_browser::tabs::{self, TabId, Tabs};
use winarchy_browser::{Blocker, address, library::Library};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::*, Threading::GetCurrentThreadId},
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

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
// WM_APP + 7 is the resident pipe wakeup, handled by the same message loop.
const THEME_CHANGED: u32 = WM_APP + 8;
const DISMISS_PALETTE: u32 = WM_APP + 9;
const LEADER_CHANGED: u32 = WM_APP + 10;
const LEADER_TIMER: usize = 0x4c44;
const TABS_CHANGED: u32 = WM_APP + 11;
const OPEN_POPUP: u32 = WM_APP + 12;
#[derive(Default)]
struct LeaderInput {
    state: Leader,
    consumed: CapturedKeys,
    actions: VecDeque<Action>,
    hook: Option<HHOOK>,
    held: HashSet<u32>,
    pass_leader_once: bool,
}
impl Drop for LeaderInput {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    }
}
const _: () = assert!(THEME_CHANGED != crate::resident::REQUEST);
#[derive(Clone)]
struct App {
    hwnd: HWND,
    picker: Rc<RefCell<Picker>>,
    leader: Rc<RefCell<LeaderInput>>,
    leader_panel: Rc<RefCell<LeaderPanel>>,
    brush: HBRUSH,
    surface_brush: HBRUSH,
    text: COLORREF,
    background: COLORREF,
    surface: COLORREF,
    controller: Option<ICoreWebView2Controller>,
    web: Option<ICoreWebView2>,
    home: bool,
    tabs: Rc<RefCell<Tabs>>,
    views: Rc<RefCell<HashMap<TabId, Rc<BrowserView>>>>,
    view_context: Option<ViewContext>,
    popups: Rc<RefCell<VecDeque<(TabId, String)>>>,
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
pub(crate) unsafe fn paint_picker(dc: HDC) {
    if let Some(app) = snapshot()
        && let Ok(picker) = app.picker.try_borrow()
    {
        picker.paint(dc);
    }
}
pub(crate) unsafe fn paint_leader(dc: HDC) {
    if let Some(app) = snapshot()
        && let Ok(panel) = app.leader_panel.try_borrow()
        && let Ok(input) = app.leader.try_borrow()
    {
        panel.paint(dc, &input.state);
    }
}
unsafe fn sync_leader(app: &App) {
    let active = app.leader.borrow().state.menu().is_some();
    if active {
        app.leader_panel.borrow_mut().notice = None;
    }
    let obsolete_hook = {
        let mut input = app.leader.borrow_mut();
        if !active && input.consumed.is_empty() {
            input.held.clear();
            input.hook.take()
        } else {
            None
        }
    };
    if let Some(hook) = obsolete_hook {
        let _ = UnhookWindowsHookEx(hook);
    }
    // Only transient feedback needs a timer; the leader itself is persistent.
    if app.leader_panel.borrow().notice.is_some() {
        SetTimer(Some(app.hwnd), LEADER_TIMER, 100, None);
    } else {
        let _ = KillTimer(Some(app.hwnd), LEADER_TIMER);
    }
    app.leader_panel
        .borrow()
        .layout(app.hwnd, app.leader.borrow().state.menu());
}
unsafe fn leader_notice(app: &App, message: &str) {
    app.leader_panel.borrow_mut().notice = Some((
        message.into(),
        Instant::now() + std::time::Duration::from_millis(1800),
    ));
    sync_leader(app);
}
// WebView2 accelerators do NOT include unmodified character keys. During a
// leader sequence only, a low-level hook captures them before they reach the
// renderer's separate input queue. It never takes focus, injects keys, records
// other applications' input or processes input outside our foreground window.
unsafe extern "system" fn leader_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some(app) = snapshot()
        && GetForegroundWindow() == app.hwnd
    {
        let key = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let repeat = {
            let mut input = app.leader.borrow_mut();
            if down {
                !input.held.insert(key.vkCode)
            } else {
                input.held.remove(&key.vkCode);
                false
            }
        };
        if leader_key(key.vkCode, key.scanCode, down, repeat, true) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
/// Only mutate keyboard state and queue UI/COM work here. Installing the
/// short-lived hook is safe in WebView's synchronous accelerator callback;
/// focus manipulation and outgoing WebView calls are not.
unsafe fn leader_key(vk: u32, scan: u32, down: bool, repeat: bool, from_hook: bool) -> bool {
    let Some(app) = snapshot() else {
        return false;
    };
    if GetForegroundWindow() != app.hwnd {
        return false;
    }
    let mut input = app.leader.borrow_mut();
    if !down {
        if let Some(suppress) = input.consumed.release(vk) {
            let _ = PostMessageW(Some(app.hwnd), LEADER_CHANGED, WPARAM(0), LPARAM(0));
            // The opening Ctrl+B down already reached the input queue before
            // its accelerator fired. Let its up reset Windows/WebView key state.
            return suppress;
        }
        return false;
    }
    if !from_hook && vk == b'B' as u32 && input.pass_leader_once {
        input.pass_leader_once = false;
        return false;
    }
    if repeat && input.consumed.contains(vk) {
        return true;
    }
    // An action can move focus to a runtime-owned control which does not send
    // its key-up through this adapter. A fresh press must not inherit that
    // consumed key or suppress normal autorepeat later.
    if !repeat {
        input.consumed.release(vk);
    }
    // Modifier transitions must pass through, notably the second Ctrl+B.
    if [
        VK_CONTROL.0 as u32,
        VK_SHIFT.0 as u32,
        VK_MENU.0 as u32,
        VK_LMENU.0 as u32,
        VK_RMENU.0 as u32,
        VK_LCONTROL.0 as u32,
        VK_RCONTROL.0 as u32,
        VK_LSHIFT.0 as u32,
        VK_RSHIFT.0 as u32,
    ]
    .contains(&vk)
    {
        return false;
    }
    // A low-level hook runs before the queued keyboard state is updated.
    let pressed = |key: VIRTUAL_KEY| {
        if from_hook {
            GetAsyncKeyState(key.0 as i32) < 0
        } else {
            GetKeyState(key.0 as i32) < 0
        }
    };
    let ctrl = pressed(VK_CONTROL);
    let alt = pressed(VK_MENU);
    let shift = pressed(VK_SHIFT);
    let win = pressed(VK_LWIN) || pressed(VK_RWIN);
    let is_leader = ctrl && !alt && !win && !shift && vk == b'B' as u32;
    if input.state.menu().is_none() && !is_leader {
        return false;
    }
    let key = if is_leader {
        Key::Leader
    } else if ctrl || alt || win {
        Key::Other
    } else if vk == VK_ESCAPE.0 as u32 {
        Key::Escape
    } else if vk == VK_BACK.0 as u32 {
        Key::Backspace
    } else {
        let mut keyboard = [0u8; 256];
        let mut chars = [0u16; 8];
        let _ = GetKeyboardState(&mut keyboard);
        if from_hook {
            for (key, down) in [(VK_CONTROL, ctrl), (VK_MENU, alt), (VK_SHIFT, shift)] {
                keyboard[key.0 as usize] =
                    (keyboard[key.0 as usize] & 1) | if down { 0x80 } else { 0 };
            }
        }
        // Flag 4 avoids changing dead-key state (Windows 10+).
        let count = ToUnicodeEx(
            vk,
            scan,
            &keyboard,
            &mut chars,
            4,
            Some(GetKeyboardLayout(0)),
        );
        if count == 1 {
            char::from_u32(chars[0] as u32)
                .map(|c| Key::Character(c.to_ascii_lowercase()))
                .unwrap_or(Key::Other)
        } else {
            Key::Other
        }
    };
    let was_active = input.state.menu().is_some();
    let outcome = input.state.input(key, repeat);
    let handled = outcome != Outcome::Pass;
    if from_hook && was_active && key == Key::Leader && !handled {
        // The hook passes the real second chord through. Its later native/
        // WebView accelerator delivery must not start another leader session.
        input.pass_leader_once = true;
    }
    if let Outcome::Execute(action) = outcome {
        input.actions.push_back(action);
    }
    if handled {
        input.consumed.press(vk, from_hook);
    }
    if input.state.menu().is_some() && input.hook.is_none() {
        input.held = (0..256)
            .filter(|key| GetAsyncKeyState(*key as i32) < 0)
            .collect();
        match GetModuleHandleW(None).and_then(|module| {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(leader_hook),
                Some(HINSTANCE(module.0)),
                0,
            )
        }) {
            Ok(hook) => input.hook = Some(hook),
            Err(error) => {
                eprintln!("Cannot capture leader keys: {error}");
                input.state.cancel();
                input.consumed.clear();
                app.leader_panel.borrow_mut().notice = Some((
                    "Keyboard capture unavailable".into(),
                    Instant::now() + std::time::Duration::from_millis(1800),
                ));
            }
        }
    }
    if handled || was_active {
        let _ = PostMessageW(Some(app.hwnd), LEADER_CHANGED, WPARAM(0), LPARAM(0));
    }
    handled
}
unsafe fn copy_url(hwnd: HWND, value: &str) -> AppResult<()> {
    use windows::Win32::System::{DataExchange::*, Memory::*};
    OpenClipboard(Some(hwnd))?;
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
    let _clipboard = Clipboard;
    let text = wide(value);
    let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2)?;
    let ptr = GlobalLock(memory);
    if ptr.is_null() {
        let _ = GlobalFree(Some(memory));
        return Err("Clipboard allocation failed".into());
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), ptr.cast::<u16>(), text.len());
    let _ = GlobalUnlock(memory);
    if let Err(error) =
        EmptyClipboard().and_then(|_| SetClipboardData(13, Some(HANDLE(memory.0))).map(|_| ()))
    {
        let _ = GlobalFree(Some(memory));
        return Err(error.into());
    }
    Ok(())
}
pub(crate) unsafe fn tabs_changed(hwnd: HWND) {
    let _ = PostMessageW(Some(hwnd), TABS_CHANGED, WPARAM(0), LPARAM(0));
}
pub(crate) unsafe fn queue_popup(source: TabId, url: String) {
    if let Some(app) = snapshot() {
        app.popups.borrow_mut().push_back((source, url));
        let _ = PostMessageW(Some(app.hwnd), OPEN_POPUP, WPARAM(0), LPARAM(0));
    }
}
pub(crate) unsafe fn tab_got_focus(id: TabId, hwnd: HWND) {
    if snapshot().is_some_and(|app| app.tabs.borrow().active() == Some(id)) {
        let _ = PostMessageW(Some(hwnd), DISMISS_PALETTE, WPARAM(0), LPARAM(0));
    }
}
unsafe fn queue_action(action: Action) {
    if let Some(app) = snapshot() {
        app.leader.borrow_mut().actions.push_back(action);
        let _ = PostMessageW(Some(app.hwnd), LEADER_CHANGED, WPARAM(0), LPARAM(0));
    }
}
unsafe fn tab_shortcut(key: u32) -> Option<Action> {
    if GetKeyState(VK_CONTROL.0 as i32) >= 0 || GetKeyState(VK_MENU.0 as i32) < 0 {
        return None;
    }
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    match (key, shift) {
        (84, false) => Some(Action::NewTab),
        (84, true) => Some(Action::ReopenTab),
        (87, false) => Some(Action::CloseTab),
        (9, false) => Some(Action::NextTab),
        (9, true) => Some(Action::PreviousTab),
        (65, true) => Some(Action::SelectTab),
        _ => None,
    }
}
pub(crate) unsafe fn web_accelerator(
    id: TabId,
    args: &ICoreWebView2AcceleratorKeyPressedEventArgs,
) -> windows::core::Result<()> {
    let Some(app) = snapshot().filter(|app| app.tabs.borrow().active() == Some(id)) else {
        return Ok(());
    };
    let mut event = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
    args.KeyEventKind(&mut event)?;
    let down = event == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
        || event == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN;
    let mut key = 0;
    args.VirtualKey(&mut key)?;
    let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
    args.PhysicalKeyStatus(&mut status)?;
    if leader_key(
        key,
        status.ScanCode,
        down,
        status.WasKeyDown.as_bool(),
        false,
    ) {
        args.SetHandled(true)?;
        return Ok(());
    }
    if !down {
        return Ok(());
    }
    if let Some(action) = tab_shortcut(key) {
        args.SetHandled(true)?;
        if !status.WasKeyDown.as_bool() {
            queue_action(action);
        }
    } else if GetKeyState(VK_CONTROL.0 as i32) < 0 && key == b'L' as u32 {
        args.SetHandled(true)?;
        PostMessageW(Some(app.hwnd), PALETTE, WPARAM(0), LPARAM(0))?;
    } else if GetKeyState(VK_CONTROL.0 as i32) < 0 && key == b'D' as u32 {
        args.SetHandled(true)?;
        if !status.WasKeyDown.as_bool() {
            PostMessageW(Some(app.hwnd), BOOKMARK, WPARAM(0), LPARAM(0))?;
        }
    } else if GetKeyState(VK_MENU.0 as i32) < 0
        && (key == VK_LEFT.0 as u32 || key == VK_RIGHT.0 as u32)
    {
        args.SetHandled(true)?;
        PostMessageW(Some(app.hwnd), HISTORY, WPARAM(key as usize), LPARAM(0))?;
    }
    Ok(())
}
unsafe fn activate_tab(id: TabId, keep_picker: bool) -> AppResult<()> {
    let app = snapshot().ok_or("Browser closed")?;
    let view = app.views.borrow().get(&id).cloned().ok_or("Unknown tab")?;
    let home = app.tabs.borrow().get(id).ok_or("Unknown tab")?.home;
    app.tabs.borrow_mut().activate(id, tabs::now());
    let views: Vec<_> = app.views.borrow().values().cloned().collect();
    for other in views {
        other.controller.SetIsVisible(false)?;
    }
    APP.with(|state| {
        let mut state = state.borrow_mut();
        let app = state.as_mut().unwrap();
        app.controller = Some(view.controller.clone());
        app.web = Some(view.web.clone());
        app.home = home;
    });
    let app = snapshot().unwrap();
    apply_opacity(&app);
    layout(&app);
    view.controller.SetIsVisible(!home)?;
    if keep_picker {
        app.picker.borrow_mut().refresh_tabs(app.hwnd, true);
    } else {
        app.picker.borrow_mut().hide();
        if home {
            palette(true);
        } else if IsWindowVisible(app.hwnd).as_bool() {
            view.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)?;
        }
    }
    tabs_changed(app.hwnd);
    Ok(())
}
unsafe fn create_tab(target: &str, keep_picker: bool) -> AppResult<TabId> {
    let app = snapshot().ok_or("Browser closed")?;
    let context = app
        .view_context
        .as_ref()
        .ok_or("WebView environment is not ready")?;
    let id = app.tabs.borrow_mut().add(target, tabs::now());
    let view = match context.create(id) {
        Ok(view) => view,
        Err(error) => {
            app.tabs.borrow_mut().discard(id);
            // The initialization pump may have consumed WM_QUIT.
            if !IsWindow(Some(app.hwnd)).as_bool() {
                PostQuitMessage(0);
            }
            return Err(error);
        }
    };
    if !IsWindow(Some(app.hwnd)).as_bool() {
        app.tabs.borrow_mut().discard(id);
        PostQuitMessage(0);
        return Err("Browser closed during tab initialization".into());
    }
    app.views.borrow_mut().insert(id, view.clone());
    activate_tab(id, keep_picker)?;
    if target != "about:blank" {
        view.web.Navigate(PCWSTR(wide(target).as_ptr()))?;
    }
    // Controller creation pumps messages. Re-signal queued host work so a
    // message consumed by that pump cannot strand an action/popup/theme update.
    for message in [LEADER_CHANGED, OPEN_POPUP, THEME_CHANGED, resident::REQUEST] {
        let _ = PostMessageW(Some(app.hwnd), message, WPARAM(0), LPARAM(0));
    }
    Ok(id)
}
unsafe fn close_tab(id: TabId) -> AppResult<()> {
    let app = snapshot().ok_or("Browser closed")?;
    if app.tabs.borrow().get(id).is_some_and(|tab| tab.pinned) {
        leader_notice(&app, "Unpin this tab before closing it");
        return Ok(());
    }
    if app.tabs.borrow().entries().len() == 1 {
        create_tab("about:blank", false)?;
    }
    if app.tabs.borrow_mut().close(id).is_none() {
        return Ok(());
    }
    let removed = app.views.borrow_mut().remove(&id);
    drop(removed); // Close outside the RefCell borrow: COM can deliver callbacks.
    let active = app.tabs.borrow().active();
    let keep_picker = app.picker.borrow().visible && app.picker.borrow().tabs_mode;
    if let Some(active) = active {
        activate_tab(active, keep_picker)?;
    } else {
        create_tab("about:blank", false)?;
    }
    Ok(())
}
unsafe fn selected_or_active(app: &App) -> Option<TabId> {
    let picker = app.picker.borrow();
    if picker.visible && picker.tabs_mode {
        picker.selected_tab()
    } else {
        app.tabs.borrow().active()
    }
}
unsafe fn bookmark(app: &App) -> AppResult<()> {
    if app.home {
        return Ok(());
    }
    let web = app.web.as_ref().ok_or("No active page")?;
    let source = take_string(|s| web.Source(s))?;
    let title = take_string(|s| web.DocumentTitle(s)).unwrap_or_default();
    let library = app.picker.borrow().library.clone();
    let result = library.borrow_mut().add_bookmark(&source, &title)?;
    if let Some(added) = result {
        eprintln!("metric bookmark_added={added}");
        if app.picker.borrow().visible && !app.picker.borrow().tabs_mode {
            app.picker.borrow_mut().refresh(app.hwnd);
            app.picker.borrow().status(if added {
                "★ Bookmark added"
            } else {
                "★ Already bookmarked"
            });
        }
    }
    Ok(())
}
unsafe fn execute_leader(
    action: Action,
    app: &App,
    env: &ICoreWebView2Environment,
    blocker: &Rc<RefCell<Blocker>>,
    filters: &std::path::Path,
) -> AppResult<()> {
    match action {
        Action::NewTab => {
            create_tab("about:blank", false)?;
            return Ok(());
        }
        Action::SelectTab => {
            app.picker.borrow_mut().show_tabs(app.hwnd);
            layout(app);
            return Ok(());
        }
        Action::CloseTab => {
            if let Some(id) = selected_or_active(app) {
                close_tab(id)?;
            }
            return Ok(());
        }
        Action::PreviousTab | Action::NextTab => {
            let id = app
                .tabs
                .borrow()
                .adjacent(if action == Action::PreviousTab { -1 } else { 1 });
            if let Some(id) = id {
                activate_tab(id, false)?;
            }
            return Ok(());
        }
        Action::ReopenTab => {
            let closed = app.tabs.borrow().recently_closed().cloned();
            if let Some(closed) = closed {
                let id = create_tab(
                    if closed.home {
                        "about:blank"
                    } else {
                        &closed.url
                    },
                    false,
                )?;
                app.tabs.borrow_mut().finish_reopen();
                let view = app.views.borrow().get(&id).cloned().unwrap();
                view.web
                    .cast::<ICoreWebView2_8>()?
                    .SetIsMuted(closed.muted)?;
            }
            return Ok(());
        }
        Action::DuplicateTab => {
            let tab = selected_or_active(app).and_then(|id| app.tabs.borrow().get(id).cloned());
            if let Some(tab) = tab {
                create_tab(if tab.home { "about:blank" } else { &tab.url }, false)?;
            }
            return Ok(());
        }
        Action::PinTab => {
            if let Some(id) = selected_or_active(app) {
                app.tabs.borrow_mut().toggle_pin(id);
                tabs_changed(app.hwnd);
            }
            return Ok(());
        }
        Action::MuteTab => {
            if let Some(id) = selected_or_active(app) {
                let muted = app.tabs.borrow().get(id).is_some_and(|tab| tab.muted);
                let view = app.views.borrow().get(&id).cloned().ok_or("Unknown tab")?;
                view.web.cast::<ICoreWebView2_8>()?.SetIsMuted(!muted)?;
                if let Some(tab) = app.tabs.borrow_mut().get_mut(id) {
                    tab.muted = !muted;
                }
                tabs_changed(app.hwnd);
            }
            return Ok(());
        }
        _ => {}
    }
    let web = app.web.as_ref().ok_or("WebView is not ready")?;
    let controller = app.controller.as_ref().ok_or("Controller is not ready")?;
    if action == Action::Address {
        palette(true);
        return Ok(());
    }
    if action == Action::Home {
        home_mode(true);
        web.Navigate(w!("about:blank"))?;
        return Ok(());
    }
    if app.home {
        leader_notice(app, "Open a page to use this action");
        return Ok(());
    }
    match action {
        Action::Address
        | Action::Home
        | Action::NewTab
        | Action::CloseTab
        | Action::PreviousTab
        | Action::NextTab
        | Action::SelectTab
        | Action::ReopenTab
        | Action::DuplicateTab
        | Action::PinTab
        | Action::MuteTab => unreachable!(),
        Action::Back => {
            web.GoBack()?;
        }
        Action::Forward => {
            web.GoForward()?;
        }
        Action::Reload => {
            web.Reload()?;
        }
        Action::Stop => {
            web.Stop()?;
        }
        Action::Bookmark => {
            // Execute against this tab before the next queued switch action.
            bookmark(app)?;
        }
        Action::Find => {
            let find = web.cast::<ICoreWebView2_28>()?.Find()?;
            let options = env
                .cast::<ICoreWebView2Environment15>()?
                .CreateFindOptions()?;
            options.SetFindTerm(w!(""))?;
            options.SetSuppressDefaultFindDialog(false)?;
            palette(false);
            find.Start(
                &options,
                &FindStartCompletedHandler::create(Box::new(move |result| {
                    if let Err(error) = result {
                        eprintln!("Cannot open find: {error}");
                        if let Some(app) = snapshot() {
                            leader_notice(&app, "Find unavailable in this runtime");
                        }
                    }
                    Ok(())
                })),
            )?;
        }
        Action::CopyUrl => {
            copy_url(app.hwnd, &take_string(|s| web.Source(s))?)?;
            leader_notice(app, "URL copied");
        }
        Action::HardReload => {
            web.CallDevToolsProtocolMethod(
                w!("Page.reload"),
                w!("{\"ignoreCache\":true}"),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                    if let Err(error) = result {
                        eprintln!("Cannot hard reload: {error}");
                        if let Some(app) = snapshot() {
                            leader_notice(&app, "Reload without cache failed");
                        }
                    }
                    Ok(())
                })),
            )?;
        }
        Action::DevTools => {
            web.OpenDevToolsWindow()?;
        }
        Action::ZoomIn | Action::ZoomOut | Action::ZoomReset => {
            let mut zoom = 1.0;
            controller.ZoomFactor(&mut zoom)?;
            controller.SetZoomFactor(match action {
                Action::ZoomIn => (zoom * 1.2).min(5.0),
                Action::ZoomOut => (zoom / 1.2).max(0.25),
                _ => 1.0,
            })?;
        }
        Action::Blocking => {
            let source = take_string(|s| web.Source(s))?;
            blocker.borrow_mut().toggle(&source, filters)?;
            let enabled = blocker.borrow().enabled(&source);
            web.Reload()?;
            leader_notice(
                app,
                if enabled {
                    "Blocking enabled for this site"
                } else {
                    "Blocking disabled for this site"
                },
            );
        }
    }
    Ok(())
}
unsafe fn layout(app: &App) {
    let mut rect = RECT::default();
    let _ = GetClientRect(app.hwnd, &mut rect);
    if let Some(c) = &app.controller {
        let _ = c.SetBounds(rect);
    }
    // Keep the native palette above the WebView child without resizing the page.
    app.picker.borrow().layout(app.hwnd);
    app.leader_panel
        .borrow()
        .layout(app.hwnd, app.leader.borrow().state.menu());
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
        app.picker.borrow_mut().set_theme(theme);
        app.leader_panel.borrow_mut().set_theme(theme);
        let _ = RedrawWindow(Some(app.hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
        let views: Vec<_> = app.views.borrow().values().cloned().collect();
        for view in views {
            let controller = view.controller.cast::<ICoreWebView2Controller2>()?;
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
            let id = a.tabs.borrow().active();
            if let Some(id) = id
                && let Some(tab) = a.tabs.borrow_mut().get_mut(id)
            {
                tab.home = home;
                if home {
                    tab.url = "about:blank".into();
                    tab.title.clear();
                }
            }
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
/// Route input inside the foreground home or open palette. Re-target key events
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
    let panel = app.picker.borrow().panel;
    if (!app.home
        && (!app.picker.borrow().visible
            || (msg.hwnd != panel && !IsChild(panel, msg.hwnd).as_bool())))
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
        WM_TIMER if wp.0 == LEADER_TIMER => {
            if let Some(app) = snapshot() {
                if app
                    .leader_panel
                    .borrow()
                    .notice
                    .as_ref()
                    .is_some_and(|(_, until)| Instant::now() >= *until)
                {
                    app.leader_panel.borrow_mut().notice = None;
                }
                sync_leader(&app);
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xffff == WA_INACTIVE as usize => {
            if let Some(app) = snapshot() {
                let mut input = app.leader.borrow_mut();
                input.state.cancel();
                input.consumed.clear();
                input.actions.clear();
                input.pass_leader_once = false;
                drop(input);
                app.leader_panel.borrow_mut().notice = None;
                sync_leader(&app);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
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
                app.leader_panel.borrow_mut().set_font(hwnd);
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
        WM_DRAWITEM => {
            if wp.0 == LIST_ID
                && let Some(app) = snapshot()
                && let Ok(picker) = app.picker.try_borrow()
            {
                picker.draw_item(&*(lp.0 as *const DRAWITEMSTRUCT));
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            let notification = (wp.0 >> 16) & 0xffff;
            if id == EDIT_ID && notification == EN_CHANGE as usize {
                let _ = PostMessageW(Some(hwnd), PICKER_CHANGED, WPARAM(0), LPARAM(0));
            } else if id == LIST_ID && notification == LBN_SELCHANGE as usize {
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
                if picker.tabs_mode {
                    picker.refresh_tabs(hwnd, true);
                } else {
                    if let Err(e) = picker.library.borrow_mut().reload() {
                        eprintln!("Cannot reload history: {e}");
                    }
                    picker.refresh(hwnd);
                }
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
        DISMISS_PALETTE => {
            if let Some(app) = snapshot()
                && !app.home
                && app.picker.borrow().visible
                && app.leader.borrow().state.menu().is_none()
                && GetFocus() != app.picker.borrow().edit
                && GetFocus() != app.picker.borrow().list
            {
                app.picker.borrow_mut().hide();
            }
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
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

type FilterStamp = Vec<Option<(u64, std::time::SystemTime)>>;
type BrowserLibraries = (Rc<RefCell<Blocker>>, Rc<RefCell<Library>>);
struct CachedBlocker { dir: PathBuf, stamp: FilterStamp, value: Rc<RefCell<Blocker>> }
/// Resident resources outlive controllers/pages. Cached COM interfaces are
/// released before the owning UI thread's apartment is uninitialized.
pub struct Resources {
    env: Option<ICoreWebView2Environment>,
    blocker: Option<CachedBlocker>,
    library: Option<(PathBuf, Rc<RefCell<Library>>)>,
    _apartment: Apartment,
}
unsafe fn release_window() {
    if let Some(app) = APP.with(|state| state.borrow_mut().take()) {
        // Also cover initialization errors, which bypass the normal host loop.
        unsafe {
            if IsWindow(Some(app.hwnd)).as_bool() { let _ = DestroyWindow(app.hwnd); }
            let views = std::mem::take(&mut *app.views.borrow_mut());
            drop(views);
            let _ = DeleteObject(app.brush.into());
            let _ = DeleteObject(app.surface_brush.into());
        }
    }
}
impl Drop for Resources {
    fn drop(&mut self) { unsafe { release_window(); } }
}
impl Resources {
    pub fn new() -> AppResult<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?; }
        Ok(Self { env: None, blocker: None, library: None, _apartment: Apartment })
    }
    fn libraries(&mut self, dir: &std::path::Path) -> AppResult<BrowserLibraries> {
        let mut stamp = Vec::new();
        for name in ["easylist.txt", "easyprivacy.txt", "custom.txt", "exceptions.json"] {
            stamp.push(match std::fs::metadata(dir.join(name)) {
                Ok(meta) => Some((meta.len(), meta.modified()?)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            });
        }
        if self.blocker.as_ref().is_none_or(|cached| cached.dir != dir || cached.stamp != stamp) {
            self.blocker = Some(CachedBlocker { dir: dir.to_owned(), stamp, value: Rc::new(RefCell::new(Blocker::load(dir)?)) });
        }
        if self.library.as_ref().is_none_or(|(path, _)| path != dir) {
            self.library = Some((dir.to_owned(), Rc::new(RefCell::new(Library::load(dir)?))));
        }
        let blocker = self.blocker.as_ref().unwrap().value.clone();
        blocker.borrow_mut().blocked = 0;
        let library = self.library.as_ref().unwrap().1.clone();
        library.borrow_mut().reload()?;
        Ok((blocker, library))
    }
}
pub fn run_demo(data: &std::path::Path) -> AppResult<Exit> {
    run_inner("", false, None, |_| {}, Some(data), &mut Resources::new()?)
}
pub fn run(
    input: &str,
    hidden: bool,
    requests: Option<&std::sync::mpsc::Receiver<Request>>,
    ready: impl FnOnce(u32),
) -> AppResult<Exit> {
    run_prepared(input, hidden, requests, ready, &mut Resources::new()?)
}
pub fn run_prepared(
    input: &str,
    hidden: bool,
    requests: Option<&std::sync::mpsc::Receiver<Request>>,
    ready: impl FnOnce(u32),
    resources: &mut Resources,
) -> AppResult<Exit> {
    run_inner(input, hidden, requests, ready, None, resources)
}
fn run_inner(
    input: &str,
    hidden: bool,
    requests: Option<&std::sync::mpsc::Receiver<Request>>,
    ready: impl FnOnce(u32),
    demo: Option<&std::path::Path>,
    resources: &mut Resources,
) -> AppResult<Exit> {
    unsafe {
        let started = Instant::now();
        let target = address(input);
        let start_home = target == "about:blank";
        let home = winarchy_theme::config_home();
        let filters = demo.unwrap_or(&home).join("browser");
        std::fs::create_dir_all(&filters)?;
        let (blocker, library) = resources.libraries(&filters)?;
        eprintln!("metric filters_ready_ms={}", started.elapsed().as_millis());
        let theme = winarchy_theme::Theme::current(&home);
        let profile = if let Some(data) = demo {
            data.join("profile")
        } else {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or("LOCALAPPDATA is missing")?
                .join("Winarchy/browser/profile")
        };
        std::fs::create_dir_all(&profile)?;
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
        let tabs = Rc::new(RefCell::new(Tabs::default()));
        let picker = Rc::new(RefCell::new(Picker::new(
            hwnd,
            instance,
            library.clone(),
            tabs.clone(),
        )?));
        let edit = picker.borrow().edit;
        let list = picker.borrow().list;
        let leader_panel = Rc::new(RefCell::new(LeaderPanel::new(hwnd, instance)?));
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                picker: picker.clone(),
                leader: Rc::new(RefCell::new(LeaderInput::default())),
                leader_panel,
                brush,
                surface_brush,
                text: color(&theme.text),
                background: color(&theme.background),
                surface: color(&theme.surface),
                controller: None,
                web: None,
                home: start_home,
                tabs: tabs.clone(),
                views: Rc::new(RefCell::new(HashMap::new())),
                view_context: None,
                popups: Rc::new(RefCell::new(VecDeque::new())),
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
        let env = if let Some(env) = &resources.env {
            env.clone()
        } else {
        let (tx, rx) = std::sync::mpsc::channel();
        let profile = wide(&profile.to_string_lossy());
        // Keep WebView-owned UI (find, context menus, dialogs) in English too.
        // This does not translate page content or change the system language.
        let options: ICoreWebView2EnvironmentOptions =
            CoreWebView2EnvironmentOptions::default().into();
        options.SetLanguage(w!("en-US"))?;
        if demo.is_some() {
            options.SetAdditionalBrowserArguments(w!(
                "--disable-background-networking --disable-sync --no-first-run"
            ))?;
        }
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR(profile.as_ptr()),
                    &options,
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
        resources.env = Some(env.clone());
        env
        };
        let context = ViewContext {
            hwnd,
            env: env.clone(),
            blocker: blocker.clone(),
            library: library.clone(),
            tabs: tabs.clone(),
            started,
        };
        APP.with(|a| a.borrow_mut().as_mut().unwrap().view_context = Some(context));
        create_tab(&target, false)?;
        eprintln!("metric webview_ready_ms={}", started.elapsed().as_millis());
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
        if demo.is_some() {
            SetPropW(
                hwnd,
                w!("WinarchyDemoReady"),
                Some(HANDLE(std::ptr::dangling_mut())),
            )?;
        }
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0).0;
            if result <= 0 {
                break;
            }
            let web = snapshot()
                .and_then(|app| app.web)
                .ok_or("No active WebView")?;
            if msg.message == TABS_CHANGED {
                if let Some(app) = snapshot() {
                    let active = app
                        .tabs
                        .borrow()
                        .active()
                        .and_then(|id| app.tabs.borrow().get(id).cloned());
                    if let Some(tab) = active {
                        if tab.home != app.home {
                            let keep = app.picker.borrow().visible && app.picker.borrow().tabs_mode;
                            if let Err(error) = activate_tab(tab.id, keep) {
                                eprintln!("Cannot update active tab: {error}");
                            }
                        }
                        let _ = SetWindowTextW(
                            hwnd,
                            PCWSTR(wide(&format!("{} — Winarchy Browser", tab.label())).as_ptr()),
                        );
                    }
                    if app.picker.borrow().visible && app.picker.borrow().tabs_mode {
                        app.picker.borrow_mut().refresh_tabs(hwnd, true);
                    }
                }
                continue;
            }
            if msg.message == OPEN_POPUP {
                if let Some(app) = snapshot() {
                    loop {
                        let popup = app.popups.borrow_mut().pop_front();
                        let Some((source, url)) = popup else {
                            break;
                        };
                        let source_exists = app.tabs.borrow().get(source).is_some();
                        if source_exists && let Err(error) = create_tab(&url, false) {
                            eprintln!("Cannot open popup tab: {error}");
                        }
                    }
                }
                continue;
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
                                    let target = address(&input);
                                    home_mode(target == "about:blank");
                                    opened = true;
                                    let _ = ShowWindow(hwnd, SW_SHOW);
                                    let _ = SetForegroundWindow(hwnd);
                                    if target == "about:blank" { let _ = SetFocus(Some(edit)); }
                                    // The prepared page is already blank. Avoid a
                                    // second navigation (and its callbacks) for home.
                                    if target != "about:blank" {
                                        web.Navigate(PCWSTR(wide(&target).as_ptr())).map_err(|e| e.to_string())?;
                                    }
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
            if msg.message == LEADER_CHANGED {
                if let Some(app) = snapshot() {
                    sync_leader(&app);
                    loop {
                        let action = app.leader.borrow_mut().actions.pop_front();
                        let Some(action) = action else {
                            break;
                        };
                        let current = snapshot().ok_or("Browser closed")?;
                        if let Err(error) =
                            execute_leader(action, &current, &env, &blocker, &filters)
                        {
                            eprintln!("Leader action failed: {error}");
                            leader_notice(&app, "Action unavailable or failed");
                        }
                    }
                }
                continue;
            }
            // WebView starts the leader through its accelerator callback; the
            // temporary hook captures subsequent unmodified keys. Native controls
            // are intercepted before TranslateMessage (no stray WM_CHAR).
            if matches!(
                msg.message,
                WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
            ) && (msg.hwnd == hwnd
                || msg.hwnd == edit
                || msg.hwnd == list
                || msg.hwnd == picker.borrow().panel)
                && leader_key(
                    msg.wParam.0 as u32,
                    ((msg.lParam.0 >> 16) & 0xff) as u32,
                    matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN),
                    msg.lParam.0 & (1 << 30) != 0,
                    false,
                )
            {
                continue;
            }
            if msg.message == WM_KEYDOWN
                && (msg.hwnd == hwnd
                    || msg.hwnd == edit
                    || msg.hwnd == list
                    || msg.hwnd == picker.borrow().panel)
                && let Some(action) = tab_shortcut(msg.wParam.0 as u32)
            {
                if msg.lParam.0 & (1 << 30) == 0 {
                    queue_action(action);
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
                if let Some(app) = snapshot()
                    && let Err(error) = bookmark(&app)
                {
                    eprintln!("Cannot save bookmark: {error}");
                    picker.borrow().status("Unable to save bookmark");
                }
                continue;
            }
            if msg.message == SUBMIT || (picker_key && msg.wParam.0 == VK_RETURN.0 as usize) {
                if picker.borrow().tabs_mode {
                    let id = picker.borrow().selected_tab();
                    if let Some(id) = id
                        && let Err(error) = activate_tab(id, false)
                    {
                        eprintln!("Cannot activate tab: {error}");
                    }
                    continue;
                }
                let input = picker.borrow().input();
                if input.trim() == ":block" {
                    let source = take_string(|s| web.Source(s))?;
                    let result = blocker.borrow_mut().toggle(&source, &filters);
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
                    web.Navigate(PCWSTR(wide(&target).as_ptr()))?;
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
                        if picker.borrow().tabs_mode {
                            if snapshot().is_some_and(|a| a.home) {
                                palette(true);
                            } else {
                                palette(false);
                            }
                        } else if snapshot().is_some_and(|a| a.home) {
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
        release_window();
        Ok(if quit { Exit::Quit } else { Exit::Closed })
    }
}
