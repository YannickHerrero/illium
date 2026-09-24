use super::{Event, EventSender, native};
use crate::{config::Config, layout::Rect, model::Model};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Model as _, ModelRc, VecModel};
use std::rc::Rc;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
slint::include_modules!();
#[cfg(test)]
mod bar_hints_tests;
#[cfg(test)]
mod bar_tests;
#[cfg(test)]
mod native_surface_tests;
pub(super) fn color(s: &str) -> slint::Color {
    let c = u32::from_str_radix(&s[1..], 16).unwrap_or_default();
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
pub(super) fn id(w: &slint::Window) -> isize {
    match w.window_handle().window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}
const ORIGINAL_PROC: &str = "WinarchyOriginalProc";
/// Windows paints a classic caption over the top of a winit window each time
/// it is activated, even without WS_CAPTION, and leaves it there until the
/// next redraw. Answering the non-client messages ourselves prevents that.
/// Marks the background surfaces, which must stay at the bottom of the Z order.
const BOTTOM: &str = "WinarchyBottom";
unsafe extern "system" fn surface_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_NCACTIVATE {
        return LRESULT(1);
    }
    if m == WM_NCPAINT {
        return LRESULT(0);
    }
    // Passive backgrounds/bars must not activate (and rise over clients), but
    // interactive launcher/picker surfaces must be able to regain keyboard focus.
    if m == WM_MOUSEACTIVATE
        && unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } & WS_EX_NOACTIVATE.0 as isize != 0
    {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe {
        if m == WM_WINDOWPOSCHANGING {
            let bottom = native::wide(BOTTOM);
            if !GetPropW(h, windows::core::PCWSTR(bottom.as_ptr())).is_invalid() {
                let pos = &mut *(l.0 as *mut WINDOWPOS);
                pos.hwndInsertAfter = HWND_BOTTOM;
                pos.flags &= !SWP_NOZORDER;
            }
        }
        let key = native::wide(ORIGINAL_PROC);
        let original = GetPropW(h, windows::core::PCWSTR(key.as_ptr())).0 as isize;
        if original == 0 {
            return DefWindowProcW(h, m, w, l);
        }
        let original: WNDPROC = std::mem::transmute::<isize, WNDPROC>(original);
        CallWindowProcW(original, h, m, w, l)
    }
}
pub(super) fn tool(w: &slint::Window, no_activate: bool) {
    unsafe {
        if id(w) == 0 {
            return;
        }
        let h = native::hwnd(id(w));
        native::corners(id(w), true);
        let key = native::wide(ORIGINAL_PROC);
        if GetPropW(h, windows::core::PCWSTR(key.as_ptr())).is_invalid() {
            let previous = SetWindowLongPtrW(h, GWLP_WNDPROC, surface_proc as *const () as isize);
            let _ = SetPropW(
                h,
                windows::core::PCWSTR(key.as_ptr()),
                Some(HANDLE(previous as *mut _)),
            );
        }
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(
            h,
            GWL_EXSTYLE,
            ((ex & !(WS_EX_NOACTIVATE.0 as isize))
                | WS_EX_TOOLWINDOW.0 as isize
                | if no_activate {
                    WS_EX_NOACTIVATE.0 as isize
                } else {
                    0
                })
                & !(WS_EX_APPWINDOW.0 as isize),
        );
        // winit keeps WS_CAPTION on frameless windows for Aero Snap, which makes
        // Windows clamp the bar to SM_CYMINTRACK (47 px at 125%).
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        SetWindowLongPtrW(
            h,
            GWL_STYLE,
            (style | WS_POPUP.0 as isize)
                & !((WS_CAPTION.0
                    | WS_THICKFRAME.0
                    | WS_SYSMENU.0
                    | WS_MINIMIZEBOX.0
                    | WS_MAXIMIZEBOX.0) as isize),
        );
        let _ = SetWindowPos(
            h,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}
#[derive(Clone)]
pub struct App {
    pub name: String,
    pub target: String,
    pub shortcut: bool,
    /// Companion application name: launched with `Command::App` instead of the target.
    pub app: Option<String>,
}
/// Persistent bar models. Replacing a model recreates every module element,
/// which loses a click whose press and release straddle a refresh; rows are
/// updated in place instead.
struct BarModels {
    workspaces: Rc<VecModel<i32>>,
    before_workspaces: Rc<VecModel<StatusItem>>,
    left: Rc<VecModel<StatusItem>>,
    center: Rc<VecModel<StatusItem>>,
    right: Rc<VecModel<StatusItem>>,
}
/// Keep the workspace group at its configured position, not always first.
/// Without a workspace entry, all left modules retain their ordinary order.
fn split_workspaces(modules: &[String]) -> (&[String], &[String]) {
    match modules.iter().position(|name| name == "workspaces") {
        Some(index) => (&modules[..index], &modules[index + 1..]),
        None => (&[], modules),
    }
}
/// Expand the single `drawer` token into its folded modules, left of the
/// chevron, while the drawer is open; the chevron alone otherwise.
fn drawer_rows(modules: &[String], drawer: &[String], expanded: bool) -> Vec<String> {
    let mut out = Vec::with_capacity(modules.len() + drawer.len());
    for name in modules {
        if name == "drawer" && expanded {
            out.extend(drawer.iter().cloned());
        }
        out.push(name.clone());
    }
    out
}
fn sync<T: Clone + PartialEq + 'static>(model: &Rc<VecModel<T>>, rows: &[T]) {
    for (i, row) in rows.iter().enumerate() {
        match model.row_data(i) {
            Some(current) if current == *row => {}
            Some(_) => model.set_row_data(i, row.clone()),
            None => model.push(row.clone()),
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum MetaMenu {
    Apps,
    System,
    Appearance,
}
#[derive(Clone)]
pub enum MetaEntry {
    Menu(MetaMenu),
    Run(crate::command::Command),
    /// Runs with the menu left open, so the setting can be stepped repeatedly.
    Adjust(crate::command::Command),
}
pub(super) mod bar_hints;
pub(super) mod expose;
pub(super) mod keybindings;
pub(super) mod lockscreen;
pub(super) mod space_picker;
pub(super) mod theme_picker;
mod wallpaper;
#[cfg(test)]
mod wallpaper_tests;
pub(super) mod workspace_switcher;
/// Public Slint positioning also updates winit's attributes before a HWND
/// exists, avoiding a first frame at the backend's default position.
pub(super) fn prepare(window: &slint::Window, r: Rect, passive: bool) {
    window.set_position(slint::PhysicalPosition::new(r.x, r.y));
    window.set_size(slint::PhysicalSize::new(
        r.w.max(1) as u32,
        r.h.max(1) as u32,
    ));
    if id(window) != 0 {
        tool(window, passive);
    }
}
pub(super) fn prewarm<T: ComponentHandle + 'static>(component: &T) {
    let window = component.window();
    if id(window) != 0 {
        return;
    } // Already prepared/opened once.
    prepare(
        window,
        Rect {
            x: -32000,
            y: -32000,
            w: 1,
            h: 1,
        },
        true,
    );
    if let Err(error) = component.show() {
        tracing::warn!(%error, "surface prewarm failed");
        return;
    }
    finish_prewarm(component.as_weak(), 0);
}
fn finish_prewarm<T: ComponentHandle + 'static>(weak: slint::Weak<T>, attempts: u8) {
    // Winit creates the HWND on a later loop turn. Hiding synchronously would
    // cancel creation entirely. A real open supersedes this offscreen request.
    slint::Timer::single_shot(std::time::Duration::from_millis(1), move || {
        let Some(component) = weak.upgrade() else {
            return;
        };
        let window = component.window();
        let position = window.position();
        if id(window) != 0 && (position.x != -32000 || position.y != -32000) {
            return;
        }
        if id(window) == 0 && attempts < 50 {
            finish_prewarm(weak, attempts + 1);
            return;
        }
        if id(window) != 0 {
            tool(window, true);
        }
        let _ = component.hide();
    });
}
pub struct Shell {
    prewarm_stage: usize,
    pub hints: bar_hints::Hints,
    pub picker: theme_picker::Picker,
    pub editor: keybindings::Editor,
    pub expose: expose::Expose,
    pub workspace_switcher: workspace_switcher::Switcher,
    pub spaces: space_picker::SpacePicker,
    pub lock: lockscreen::Lock,
    surface_key: Option<(Vec<Rect>, bool, String, i32)>,
    pub backgrounds: Vec<Background>,
    pub bars: Vec<Bar>,
    /// Bars show the wallpaper through; toggled by clicking an empty bar area.
    pub bar_transparent: bool,
    models: Vec<BarModels>,
    pub launcher: Launcher,
    popup: Popup,
    /// Kind of the module whose popup is open.
    pub popup_open: Option<String>,
    popup_pending: Option<Rect>,
    popup_keyboard: bool,
    apps: Vec<App>,
    search_names: Vec<String>,
    indexed_apps: Vec<App>,
    index_generation: u64,
    index_running: bool,
    index_again: bool,
    aliases: Vec<App>,
    pub results: Vec<App>,
    pub visible: bool,
    tx: EventSender,
    pub pending: bool,
    launcher_pending: Option<Rect>,
    descriptions: bool,
    /// The launcher surface shows the session menu instead of applications.
    pub meta: bool,
    /// Submenu shown, or None at the menu root.
    meta_menu: Option<MetaMenu>,
    meta_items: Vec<(String, MetaEntry)>,
    meta_results: Vec<MetaEntry>,
    home: std::path::PathBuf,
    theme: String,
    pub wallpaper: Option<String>,
    wallpaper_images: Vec<slint::Image>,
    /// Small blurred copies of the wallpaper, one per monitor, for the exposé.
    wallpaper_blur: Vec<slint::Image>,
    wallpaper_sizes: Vec<(u32, u32)>,
    wallpaper_key: Option<crate::wallpaper::loader::Key>,
    wallpaper_loader: crate::wallpaper::loader::Loader,
    wallpaper_pending: Option<wallpaper::Pending>,
    pub wallpaper_error: Option<String>,
    wallpaper_palette_dirty: bool,
    /// Bar icons of the volume module: sound on, then muted.
    volume_icons: [slint::Image; 2],
    /// Collapsed and expanded chevrons of the bar drawer.
    drawer_icons: [slint::Image; 2],
    /// The drawer chevron was clicked open; stays until clicked again.
    pub drawer_pinned: bool,
    /// The pointer is over a bar; the drawer follows it without delay.
    pub drawer_hovered: bool,
    /// A hint session is starting: every module must be reachable.
    pub drawer_hints: bool,
}
fn scan(path: &std::path::Path, out: &mut Vec<App>) {
    for path in crate::files::shortcuts(path, 8192, 16) {
        out.push(App {
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            target: path.to_string_lossy().into(),
            shortcut: true,
            app: None,
        });
    }
}
fn score(query: &str, text: &str) -> Option<usize> {
    score_folded(&query.to_lowercase(), &text.to_lowercase())
}
fn score_folded(query: &str, text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let mut total = 0;
    for q in query.chars() {
        let (i, _) = chars.find(|(_, c)| *c == q)?;
        total += i;
    }
    Some(total)
}
#[test]
fn folded_launcher_matching_preserves_unicode_subsequence_scores() {
    assert_eq!(score("ED", "Editor"), Some(1));
    assert_eq!(score("É語", "Éditeur 日本語"), Some(15));
    assert_eq!(score_folded("", "editor"), Some(0));
    assert_eq!(score_folded("dx", "editor"), None);
}
impl Shell {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let launcher = Launcher::new().map_err(|e| e.to_string())?;
        let t = tx.clone();
        launcher.on_search(move |q| {
            let _ = t.send(Event::Search(q.into()));
        });
        let t = tx.clone();
        launcher.on_activate(move |n| {
            let _ = t.send(Event::Launch(n));
        });
        let t = tx.clone();
        launcher.on_dismiss(move || {
            let _ = t.send(Event::Dismiss);
        });
        let t = tx.clone();
        launcher.on_back(move || {
            let _ = t.send(Event::Back);
        });
        let popup = Popup::new().map_err(|e| e.to_string())?;
        Ok(Self {
            prewarm_stage: 0,
            hints: bar_hints::Hints::new()?,
            picker: theme_picker::Picker::new(tx.clone())?,
            editor: keybindings::Editor::new(tx.clone())?,
            expose: expose::Expose::new(tx.clone())?,
            workspace_switcher: workspace_switcher::Switcher::new(tx.clone())?,
            spaces: space_picker::SpacePicker::new(tx.clone())?,
            lock: lockscreen::Lock::new(tx.clone()),
            surface_key: None,
            backgrounds: vec![],
            bars: vec![],
            bar_transparent: false,
            models: vec![],
            launcher,
            popup,
            popup_open: None,
            popup_pending: None,
            popup_keyboard: false,
            apps: vec![],
            search_names: vec![],
            indexed_apps: vec![],
            index_generation: 0,
            index_running: false,
            index_again: false,
            aliases: vec![],
            results: vec![],
            visible: false,
            pending: false,
            launcher_pending: None,
            descriptions: false,
            meta: false,
            meta_menu: None,
            meta_items: vec![],
            meta_results: vec![],
            home: std::path::PathBuf::new(),
            theme: String::new(),
            wallpaper: None,
            wallpaper_images: vec![],
            wallpaper_blur: vec![],
            wallpaper_sizes: vec![],
            wallpaper_key: None,
            wallpaper_loader: crate::wallpaper::loader::Loader::default(),
            wallpaper_pending: None,
            wallpaper_error: None,
            wallpaper_palette_dirty: false,
            volume_icons: [
                slint::Image::load_from_svg_data(include_bytes!("../../ui/icons/volume.svg"))
                    .map_err(|e| e.to_string())?,
                slint::Image::load_from_svg_data(include_bytes!("../../ui/icons/volume-muted.svg"))
                    .map_err(|e| e.to_string())?,
            ],
            drawer_icons: [
                slint::Image::load_from_svg_data(include_bytes!("../../ui/icons/chevron-left.svg"))
                    .map_err(|e| e.to_string())?,
                slint::Image::load_from_svg_data(include_bytes!(
                    "../../ui/icons/chevron-right.svg"
                ))
                .map_err(|e| e.to_string())?,
            ],
            drawer_pinned: false,
            drawer_hovered: false,
            drawer_hints: false,
            tx,
        })
    }
    /// Blurred wallpaper of monitor `index`, or an empty image on a solid background.
    pub fn wallpaper_image(&self, index: usize) -> slint::Image {
        self.wallpaper_images
            .get(index)
            .cloned()
            .unwrap_or_default()
    }
    pub fn backdrop(&self, index: usize) -> slint::Image {
        self.wallpaper_blur.get(index).cloned().unwrap_or_default()
    }
    /// Whether one of the bars is the root window `id`.
    pub fn is_bar(&self, window: isize) -> bool {
        self.bars.iter().any(|b| id(b.window()) == window)
    }
    /// The drawer stays open while pinned, hovered, targeted by hints or
    /// while a popup anchored under one of its modules is showing.
    fn drawer_expanded(&self, applets: &super::applet::Runtime) -> bool {
        self.drawer_pinned
            || self.drawer_hovered
            || self.drawer_hints
            || self.hints.opened
            || self.popup_open.is_some()
            || applets.open.is_some()
    }
    fn background_opacity(c: &Config) -> f32 {
        let mut theme = c.theme.clone();
        winarchy_theme::opacity::apply(&c.home, &c.global.theme, &mut theme);
        theme.background_opacity
    }
    /// Opacity-only changes never rebuild surfaces or restart applet providers.
    pub fn apply_opacity(&self, c: &Config) {
        let opacity = Self::background_opacity(c);
        for bar in &self.bars {
            bar.set_background_opacity(opacity);
        }
    }
    /// Update existing surfaces in place; application index, geometry and UI state stay intact.
    pub fn apply_theme(&mut self, c: &Config) {
        self.apply_palette(c);
        self.refresh_wallpaper();
    }
    /// Palette-only refresh never schedules another wallpaper request.
    pub fn apply_palette(&mut self, c: &Config) {
        self.hints.close();
        self.picker.apply_theme(c);
        self.editor.apply_theme(c);
        self.expose.apply_theme(c);
        self.workspace_switcher.apply_theme(c);
        self.spaces.apply_theme(c);
        self.home = c.home.clone();
        self.theme = c.global.theme.clone();
        self.popup.set_bg(color(&c.theme.background));
        self.popup.set_fg(color(&c.theme.text));
        self.popup.set_muted(color(&c.theme.subtext));
        self.popup.set_overlay(color(&c.theme.overlay));
        for b in &self.backgrounds {
            b.set_bg(color(&c.theme.background));
        }
        for b in &self.bars {
            b.set_bg(color(&c.theme.surface));
            b.set_fg(color(&c.theme.text));
            b.set_accent(color(&c.theme.accent));
            b.set_muted(color(&c.theme.subtext));
        }
        self.apply_opacity(c);
        self.launcher.set_bg(color(&c.theme.background));
        self.launcher.set_fg(color(&c.theme.text));
        self.launcher.set_accent(color(&c.theme.accent));
        self.launcher.set_overlay(color(&c.theme.overlay));
    }
    pub fn configure(&mut self, c: &Config, monitors: &[Rect]) -> Result<(), String> {
        self.hints.close();
        self.pending = true;
        self.picker.apply_theme(c);
        self.editor.apply_theme(c);
        self.expose.apply_theme(c);
        self.workspace_switcher.apply_theme(c);
        self.spaces.apply_theme(c);
        self.descriptions = c.launcher.show_descriptions;
        self.home = c.home.clone();
        self.theme = c.global.theme.clone();
        let sizes: Vec<_> = monitors
            .iter()
            .map(|r| (r.w.max(1) as u32, r.h.max(1) as u32))
            .collect();
        if sizes != self.wallpaper_sizes {
            self.wallpaper_sizes = sizes;
            self.wallpaper_key = None;
        }
        self.refresh_wallpaper();
        let surface_key = (
            monitors.to_vec(),
            c.bar.enabled,
            c.bar.position.clone(),
            c.bar.height,
        );
        if self.surface_key.as_ref() != Some(&surface_key) {
            self.create_surfaces(c, monitors)?;
            self.surface_key = Some(surface_key);
        }
        self.apply_theme(c);
        self.configure_apps(c);
        Ok(())
    }
    fn create_surfaces(&mut self, c: &Config, monitors: &[Rect]) -> Result<(), String> {
        for b in self.bars.drain(..) {
            let _ = b.hide();
        }
        self.models.clear();
        for b in self.backgrounds.drain(..) {
            let _ = b.hide();
        }
        self.close_popup();
        self.popup.set_bg(color(&c.theme.background));
        self.popup.set_fg(color(&c.theme.text));
        self.popup.set_muted(color(&c.theme.subtext));
        self.popup.set_overlay(color(&c.theme.overlay));
        for (index, r) in monitors.iter().enumerate() {
            let b = Background::new().map_err(|e| e.to_string())?;
            b.set_bg(color(&c.theme.background));
            b.set_wallpaper(
                self.wallpaper_images
                    .get(index)
                    .cloned()
                    .unwrap_or_default(),
            );
            b.set_surface_width(super::dpi::logical(*r, r.w));
            b.set_surface_height(super::dpi::logical(*r, r.h));
            b.show().map_err(|e| e.to_string())?;
            self.backgrounds.push(b);
            if c.bar.enabled {
                let b = Bar::new().map_err(|e| e.to_string())?;
                b.set_bg(color(&c.theme.surface));
                b.set_fg(color(&c.theme.text));
                b.set_accent(color(&c.theme.accent));
                b.set_muted(color(&c.theme.subtext));
                b.set_bottom(c.bar.position == "bottom");
                b.set_transparent(self.bar_transparent);
                b.set_background_opacity(Self::background_opacity(c));
                let tx = self.tx.clone();
                b.on_toggle_background(move || {
                    let _ = tx.send(Event::BarBackground);
                });
                let tx = self.tx.clone();
                b.on_workspace(move |n| {
                    let _ = tx.send(Event::Command(
                        crate::command::Command::Workspace(n as u8),
                        None,
                    ));
                });
                let tx = self.tx.clone();
                b.on_module(move |kind, x| {
                    let _ = tx.send(Event::Module(kind.to_string(), x as i32, index));
                });
                let tx = self.tx.clone();
                b.on_hint_position(move |epoch, index, x| {
                    let _ = tx.send(Event::BarHintPosition(epoch as u32, index as usize - 1, x));
                });
                b.set_surface_width(super::dpi::logical(*r, r.w));
                b.set_surface_height(c.bar.height as f32);
                b.show().map_err(|e| e.to_string())?;
                let models = BarModels {
                    workspaces: Rc::new(VecModel::default()),
                    before_workspaces: Rc::new(VecModel::default()),
                    left: Rc::new(VecModel::default()),
                    center: Rc::new(VecModel::default()),
                    right: Rc::new(VecModel::default()),
                };
                b.set_workspaces(ModelRc::from(models.workspaces.clone()));
                b.set_before_workspace_items(ModelRc::from(models.before_workspaces.clone()));
                b.set_left_items(ModelRc::from(models.left.clone()));
                b.set_center_items(ModelRc::from(models.center.clone()));
                b.set_right_items(ModelRc::from(models.right.clone()));
                self.models.push(models);
                self.bars.push(b);
            }
        }
        Ok(())
    }
    fn configure_apps(&mut self, c: &Config) {
        self.aliases = c
            .apps
            .apps
            .iter()
            .map(|(name, target)| App {
                name: name.clone(),
                target: target.clone(),
                shortcut: false,
                app: None,
            })
            .collect();
        for (label, name) in Self::APPS {
            self.aliases.push(App {
                name: label.into(),
                target: String::new(),
                shortcut: false,
                app: Some(name.into()),
            });
        }
        self.rebuild_apps(c.launcher.max_results);
        self.reindex(); // Explicit reload also discovers installed/removed applications.
    }
    fn rebuild_apps(&mut self, max: usize) {
        self.apps = self
            .aliases
            .iter()
            .chain(&self.indexed_apps)
            .cloned()
            .collect();
        self.apps.sort_by_cached_key(|a| a.name.to_lowercase());
        self.apps.dedup_by(|a, b| a.name == b.name);
        self.search_names = self
            .apps
            .iter()
            .map(|app| app.name.to_lowercase())
            .collect();
        let query = self.launcher.get_query();
        self.search(&query, max);
    }
    fn reindex(&mut self) {
        if self.index_running {
            self.index_again = true;
            return;
        }
        self.index_running = true;
        self.index_generation = self.index_generation.wrapping_add(1);
        let generation = self.index_generation;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut apps = Vec::new();
            for env in ["APPDATA", "PROGRAMDATA"] {
                if let Some(root) = std::env::var_os(env) {
                    scan(
                        &std::path::PathBuf::from(root)
                            .join("Microsoft/Windows/Start Menu/Programs"),
                        &mut apps,
                    );
                }
            }
            apps.extend(
                native::packaged_apps()
                    .into_iter()
                    .map(|(name, target)| App {
                        name,
                        target,
                        shortcut: true,
                        app: None,
                    }),
            );
            tracing::debug!(
                elapsed_ms = started.elapsed().as_millis(),
                count = apps.len(),
                "applications indexed off UI thread"
            );
            // This is a worker, not a hook: retry a full bounded queue so the
            // single-flight completion cannot be lost during an event burst.
            let mut event = Event::AppsIndexed(generation, apps);
            loop {
                match tx.send(event) {
                    Ok(()) => break,
                    Err(std::sync::mpsc::TrySendError::Full(e)) => {
                        event = e;
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
        });
    }
    pub fn indexed(&mut self, generation: u64, apps: Vec<App>, max: usize) {
        if generation != self.index_generation {
            return;
        }
        self.index_running = false;
        self.indexed_apps = apps;
        self.rebuild_apps(max);
        if std::mem::take(&mut self.index_again) {
            self.reindex();
        }
    }
    /// At most one native surface per idle turn, without activating it.
    pub fn prewarm_step(&mut self) -> bool {
        match self.prewarm_stage {
            0 => prewarm(&self.launcher),
            1 => prewarm(&self.popup),
            2 => self.picker.prewarm(),
            3 => self.editor.prewarm(),
            4 => self.expose.prewarm(),
            5 => self.hints.prewarm(),
            6 => self.spaces.prewarm(),
            _ => return false,
        }
        self.prewarm_stage += 1;
        true
    }
    pub fn arrange(&mut self, c: &Config, monitors: &[Rect]) -> bool {
        if self.pending {
            if self.backgrounds.iter().any(|b| id(b.window()) == 0)
                || self.bars.iter().any(|b| id(b.window()) == 0)
            {
                return false;
            }
            for (b, r) in self.backgrounds.iter().zip(monitors) {
                tool(b.window(), true);
                let bottom = native::wide(BOTTOM);
                unsafe {
                    let _ = SetPropW(
                        native::hwnd(id(b.window())),
                        windows::core::PCWSTR(bottom.as_ptr()),
                        Some(HANDLE(std::ptr::dangling_mut())),
                    );
                }
                native::position(id(b.window()), *r, Some(HWND_BOTTOM));
            }
            for (b, r) in self.bars.iter().zip(monitors) {
                let height = super::dpi::scale(*r, c.bar.height);
                tool(b.window(), true);
                native::position(
                    id(b.window()),
                    Rect {
                        x: r.x,
                        y: if c.bar.position == "top" {
                            r.y
                        } else {
                            r.y + r.h - height
                        },
                        w: r.w,
                        h: height,
                    },
                    Some(HWND_TOPMOST),
                );
            }
            self.pending = false;
        }
        if let Some(r) = self.launcher_pending
            && id(self.launcher.window()) != 0
        {
            let w = super::dpi::scale(r, c.launcher.width).min(r.w);
            let h = super::dpi::scale(r, c.launcher.max_results as i32 * 38 + 65).min(r.h);
            tool(self.launcher.window(), false);
            native::position(
                id(self.launcher.window()),
                Rect {
                    x: r.x + (r.w - w) / 2,
                    y: r.y + (r.h - h) / 2,
                    w,
                    h,
                },
                Some(HWND_TOPMOST),
            );
            native::focus(id(self.launcher.window()), false);
            self.launcher.invoke_focus_search();
            self.launcher_pending = None;
        }
        if let Some(r) = self.popup_pending
            && id(self.popup.window()) != 0
        {
            tool(self.popup.window(), !self.popup_keyboard);
            native::position(id(self.popup.window()), r, Some(HWND_TOPMOST));
            if self.popup_keyboard {
                native::focus(id(self.popup.window()), false);
            }
            self.popup_pending = None;
        }
        self.picker.arrange();
        self.editor.arrange();
        self.expose.arrange();
        self.spaces.arrange();
        self.lock.arrange();
        true
    }
    pub fn toggle_bar_background(&mut self) {
        self.bar_transparent = !self.bar_transparent;
        for b in &self.bars {
            b.set_transparent(self.bar_transparent);
        }
    }
    pub fn popup_hwnd(&self) -> isize {
        id(self.popup.window())
    }
    /// Shows `lines` under the bar module centered at logical `x` on `monitor`.
    pub fn open_popup(
        &mut self,
        c: &Config,
        monitor: Rect,
        kind: String,
        x: i32,
        title: String,
        lines: Vec<String>,
    ) {
        let width = super::dpi::scale(monitor, 300);
        let height = super::dpi::scale(monitor, 52 + 22 * lines.len() as i32);
        let bar = super::dpi::scale(monitor, c.bar.height);
        let gap = super::dpi::scale(monitor, 6);
        let center = monitor.x + super::dpi::scale(monitor, x);
        let r = Rect {
            x: (center - width / 2).clamp(monitor.x, monitor.x + monitor.w - width),
            y: if c.bar.position == "top" {
                monitor.y + bar + gap
            } else {
                monitor.y + monitor.h - bar - gap - height
            },
            w: width,
            h: height,
        };
        self.popup.set_heading(title.into());
        self.popup.set_lines(ModelRc::from(Rc::new(VecModel::from(
            lines
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        ))));
        self.popup
            .set_surface_width(super::dpi::logical(monitor, width));
        self.popup
            .set_surface_height(super::dpi::logical(monitor, height));
        prepare(self.popup.window(), r, !self.popup_keyboard);
        if self.popup.show().is_err() {
            return;
        }
        self.popup_open = Some(kind);
        self.popup_pending = Some(r);
    }
    pub fn focus_popup_on_arrange(&mut self) {
        self.popup_keyboard = self.popup_open.is_some();
    }
    pub fn close_popup(&mut self) {
        self.popup_keyboard = false;
        if self.popup_open.take().is_some() {
            let _ = self.popup.hide();
        }
        self.popup_pending = None;
    }
    pub fn search(&mut self, q: &str, max: usize) {
        if self.meta {
            let mut matches: Vec<_> = self
                .meta_items
                .iter()
                .filter_map(|(name, command)| score(q, name).map(|s| (s, name, command)))
                .collect();
            matches.sort_by(|(a, name_a, _), (b, name_b, _)| {
                a.cmp(b).then_with(|| name_a.cmp(name_b))
            });
            let shown: Vec<slint::SharedString> = matches
                .iter()
                .take(max)
                .map(|(_, name, _)| (*name).clone().into())
                .collect();
            self.meta_results = matches
                .into_iter()
                .take(max)
                .map(|(_, _, command)| command.clone())
                .collect();
            self.launcher
                .set_results(ModelRc::from(Rc::new(VecModel::from(shown))));
            return;
        }
        let query = q.to_lowercase();
        let mut matches: Vec<_> = self
            .apps
            .iter()
            .zip(&self.search_names)
            .filter_map(|(a, name)| score_folded(&query, name).map(|s| (s, a)))
            .collect();
        matches
            .sort_by(|(a, app_a), (b, app_b)| a.cmp(b).then_with(|| app_a.name.cmp(&app_b.name)));
        self.results = matches
            .into_iter()
            .take(max)
            .map(|(_, a)| a.clone())
            .collect();
        self.launcher
            .set_results(ModelRc::from(Rc::new(VecModel::from(
                self.results
                    .iter()
                    .map(|a| {
                        if self.descriptions {
                            format!("{} — {}", a.name, a.target).into()
                        } else {
                            a.name.clone().into()
                        }
                    })
                    .collect::<Vec<_>>(),
            ))));
    }
    pub fn interactive(&self) -> bool {
        self.hints.opened
            || self.visible
            || self.picker.opened
            || self.editor.opened
            || self.expose.opened
            || self.workspace_switcher.opened
            || self.spaces.opened
            || self.lock.opened
    }
    pub fn dismiss(&mut self) {
        let _ = self.launcher.hide();
        self.visible = false;
        self.meta = false;
        self.launcher_pending = None;
    }
    pub fn toggle(&mut self, c: &Config, r: Rect) -> Result<(), String> {
        if self.visible {
            self.dismiss();
            return Ok(());
        }
        self.meta = false;
        self.open(c, r)
    }
    /// Session actions run through the same launcher surface. Power actions go
    /// through the OS shutdown tool resolved from the system directory.
    pub fn toggle_meta(&mut self, c: &Config, r: Rect) -> Result<(), String> {
        if self.visible {
            self.dismiss();
            return Ok(());
        }
        self.meta = true;
        self.meta_menu = None;
        self.meta_items = Self::meta_root();
        self.open(c, r)
    }
    fn meta_root() -> Vec<(String, MetaEntry)> {
        vec![
            ("Apps ›".into(), MetaEntry::Menu(MetaMenu::Apps)),
            ("System ›".into(), MetaEntry::Menu(MetaMenu::System)),
            (
                "Keybindings ›".into(),
                MetaEntry::Run(crate::command::Command::Keybindings),
            ),
            ("Appearance ›".into(), MetaEntry::Menu(MetaMenu::Appearance)),
            ("Demo".into(), MetaEntry::Run(crate::command::Command::Demo)),
        ]
    }
    fn meta_appearance(c: &Config) -> Vec<(String, MetaEntry)> {
        use crate::command::Command;
        let opacity = (Self::background_opacity(c) * 100.0).round();
        vec![
            ("Theme ›".into(), MetaEntry::Run(Command::ThemePicker)),
            (
                "Wallpaper ›".into(),
                MetaEntry::Run(Command::WallpaperPicker),
            ),
            (
                format!("Increase opacity ({opacity}%)"),
                MetaEntry::Adjust(Command::BackgroundOpacity(true)),
            ),
            (
                format!("Decrease opacity ({opacity}%)"),
                MetaEntry::Adjust(Command::BackgroundOpacity(false)),
            ),
            (
                "Reset opacity".into(),
                MetaEntry::Adjust(Command::ResetOpacity),
            ),
            (
                if c.global.background_blur {
                    "Disable blur"
                } else {
                    "Enable blur"
                }
                .into(),
                MetaEntry::Adjust(Command::ToggleBlur),
            ),
        ]
    }
    /// Companion applications, also indexed by the launcher.
    pub const APPS: [(&'static str, &'static str); 3] = [
        ("Files", "files"),
        ("Tasks", "tasks"),
        ("Screenshot", "shot"),
    ];
    fn meta_apps() -> Vec<(String, MetaEntry)> {
        Self::APPS
            .iter()
            .map(|(label, name)| {
                (
                    (*label).into(),
                    MetaEntry::Run(crate::command::Command::App((*name).into())),
                )
            })
            .collect()
    }
    fn meta_system() -> Result<Vec<(String, MetaEntry)>, String> {
        use crate::command::Command;
        let shutdown = super::security::os_executable("shutdown.exe", true)?;
        let rundll = super::security::os_executable("rundll32.exe", true)?;
        let run = |line: String| {
            MetaEntry::Run(Command::LaunchTarget {
                target: line,
                shortcut: false,
            })
        };
        Ok(vec![
            (
                "Hibernate".into(),
                run(format!("\"{}\" /h", shutdown.display())),
            ),
            (
                "Lock".into(),
                run(format!(
                    "\"{}\" user32.dll,LockWorkStation",
                    rundll.display()
                )),
            ),
            (
                "Restart".into(),
                run(format!("\"{}\" /r /t 0", shutdown.display())),
            ),
            (
                "Shut down".into(),
                run(format!("\"{}\" /s /t 0", shutdown.display())),
            ),
            if super::session::explorer_running() {
                (
                    "Stop Explorer".into(),
                    MetaEntry::Run(Command::Explorer(false)),
                )
            } else {
                (
                    "Start Explorer".into(),
                    MetaEntry::Run(Command::Explorer(true)),
                )
            },
            ("Quit Winarchy".into(), MetaEntry::Run(Command::Quit)),
        ])
    }
    fn wallpaper_names(&self) -> Result<Vec<String>, String> {
        let dir = winarchy_theme::pack::wallpaper_dir(&self.home, &self.theme)?;
        winarchy_theme::pack::images(&dir)
    }
    /// Enters a submenu or returns the command to run for result `n`, and
    /// whether the menu stays open around it.
    pub fn meta_activate(
        &mut self,
        n: usize,
        c: &Config,
    ) -> Option<(crate::command::Command, bool)> {
        match self.meta_results.get(n).cloned()? {
            MetaEntry::Run(command) => Some((command, false)),
            MetaEntry::Adjust(command) => Some((command, true)),
            MetaEntry::Menu(menu) => {
                self.meta_items = match menu {
                    MetaMenu::Apps => Self::meta_apps(),
                    MetaMenu::System => Self::meta_system().unwrap_or_default(),
                    MetaMenu::Appearance => Self::meta_appearance(c),
                };
                self.meta_menu = Some(menu);
                self.launcher.set_query("".into());
                self.launcher.set_selected(0);
                self.search("", c.launcher.max_results);
                None
            }
        }
    }
    /// Relabels the Appearance entries after an adjustment, keeping the query
    /// and the selected row.
    pub fn meta_refresh(&mut self, c: &Config) {
        if self.meta_menu == Some(MetaMenu::Appearance) {
            self.meta_items = Self::meta_appearance(c);
            let query = self.launcher.get_query();
            self.search(&query, c.launcher.max_results);
        }
    }
    /// Backspace on an empty query: back to the menu root.
    pub fn meta_back(&mut self, max: usize) {
        if self.meta && self.meta_menu.take().is_some() {
            self.meta_items = Self::meta_root();
            self.launcher.set_selected(0);
            self.search("", max);
        }
    }
    fn open(&mut self, c: &Config, r: Rect) -> Result<(), String> {
        self.launcher.set_query("".into());
        self.launcher.set_selected(0);
        self.search("", c.launcher.max_results);
        let w = super::dpi::scale(r, c.launcher.width).min(r.w);
        let h = super::dpi::scale(r, c.launcher.max_results as i32 * 38 + 65).min(r.h);
        self.launcher.set_surface_width(super::dpi::logical(r, w));
        self.launcher.set_surface_height(super::dpi::logical(r, h));
        prepare(
            self.launcher.window(),
            Rect {
                x: r.x + (r.w - w) / 2,
                y: r.y + (r.h - h) / 2,
                w,
                h,
            },
            false,
        );
        self.launcher.show().map_err(|e| e.to_string())?;
        self.launcher_pending = Some(r);
        self.visible = true;
        Ok(())
    }
    pub fn open_hints(&mut self, c: &Config, r: Rect, monitor: usize) {
        let Some(models) = self.models.get(monitor) else {
            return;
        };
        let kinds = [
            &models.before_workspaces,
            &models.left,
            &models.center,
            &models.right,
        ]
        .into_iter()
        .flat_map(|model| model.iter())
        .filter(|item| item.hint_id > 0)
        .map(|item| item.kind.to_string())
        .collect();
        self.hints.open(c, r, monitor, kinds);
        if self.hints.opened {
            self.bars[monitor].set_hint_request(self.hints.generation as i32);
        }
    }
    pub fn refresh(&self, m: &Model, c: &Config, applets: &super::applet::Runtime) {
        // Freeze both numbering and geometry for the entire selection session.
        if self.hints.opened {
            return;
        }
        let workspaces: Vec<i32> = if c.bar.left.iter().any(|s| s == "workspaces") {
            (1..=9u8)
                .filter(|n| *n == m.active || m.occupied(*n))
                .map(i32::from)
                .collect()
        } else {
            vec![]
        };
        let title = m.focused.map(native::title).unwrap_or_default();
        let expanded = self.drawer_expanded(applets);
        // Preserve configured order and repeated separators. The clock expands
        // into independently actionable time and date items.
        let items = |modules: &[String]| {
            let mut out = Vec::new();
            for name in &drawer_rows(modules, &c.bar.drawer, expanded) {
                let item = if name == "drawer" {
                    StatusItem {
                        kind: "drawer".into(),
                        has_icon: true,
                        icon: self.drawer_icons[usize::from(expanded)].clone(),
                        ..Default::default()
                    }
                } else if let Some((label, icon)) = applets.item(name) {
                    StatusItem {
                        kind: name.clone().into(),
                        value: label.into(),
                        sprite: applets.sprite(name),
                        secondary: Default::default(),
                        has_icon: icon.is_some(),
                        icon: icon.unwrap_or_default(),
                        hint_id: 0,
                        level: 0,
                        charging: false,
                    }
                } else if name == "battery" {
                    let Some((percent, plugged)) = super::status::battery_status() else {
                        continue;
                    };
                    StatusItem {
                        kind: "battery".into(),
                        value: format!("{percent}%").into(),
                        secondary: Default::default(),
                        has_icon: false,
                        icon: slint::Image::default(),
                        level: i32::from(percent),
                        charging: plugged,
                        hint_id: 0,
                        ..Default::default()
                    }
                } else if name == "volume" {
                    let Some((_, muted)) = super::audio::volume_state() else {
                        continue;
                    };
                    StatusItem {
                        kind: "volume".into(),
                        value: "".into(),
                        secondary: Default::default(),
                        has_icon: true,
                        icon: self.volume_icons[usize::from(muted)].clone(),
                        hint_id: 0,
                        level: 0,
                        charging: false,
                        ..Default::default()
                    }
                } else if name == "space" {
                    // A single space is the plain desktop: nothing to tell apart.
                    if m.spaces.len() < 2 {
                        continue;
                    }
                    StatusItem {
                        kind: "space".into(),
                        value: m.space_name().into(),
                        ..Default::default()
                    }
                } else if name == "clock" {
                    let (time, date) = super::status::clock_labels(c);
                    out.push(StatusItem {
                        kind: "time".into(),
                        value: time.into(),
                        ..Default::default()
                    });
                    // Keep the calendar's existing `attach = "clock"` contract.
                    StatusItem {
                        kind: "clock".into(),
                        value: date.into(),
                        ..Default::default()
                    }
                } else if let Some(value) = super::status::item(c, &title, name) {
                    StatusItem {
                        kind: name.clone().into(),
                        value: value.into(),
                        secondary: Default::default(),
                        hint_id: 0,
                        has_icon: false,
                        icon: slint::Image::default(),
                        level: 0,
                        charging: false,
                        ..Default::default()
                    }
                } else {
                    continue;
                };
                out.push(item);
            }
            out
        };
        let (before, after) = split_workspaces(&c.bar.left);
        let mut before_workspaces = items(before);
        let mut left = items(after);
        let mut center = items(&c.bar.center);
        let mut right = items(&c.bar.right);
        let mut hint_id = 0;
        for item in before_workspaces
            .iter_mut()
            .chain(&mut left)
            .chain(&mut center)
            .chain(&mut right)
        {
            let kind = item.kind.as_str();
            let interactive = matches!(kind, "clock" | "battery" | "cpu" | "memory" | "space")
                || applets.is_applet(kind)
                || applets.attached(kind).is_some();
            if interactive && hint_id < crate::bar_hints::LABELS.len() as i32 {
                hint_id += 1;
                item.hint_id = hint_id;
            }
        }
        for (b, models) in self.bars.iter().zip(&self.models) {
            b.set_active(m.active as i32);
            b.set_japanese_workspace_numbers(c.bar.japanese_workspace_numbers);
            b.set_workspace_font_family(c.bar.workspace_font_family.as_str().into());
            b.set_workspace_font_size(c.bar.workspace_font_size as f32);
            b.set_workspace_font_weight(c.bar.workspace_font_weight.unwrap_or(0));
            sync(&models.workspaces, &workspaces);
            sync(&models.before_workspaces, &before_workspaces);
            sync(&models.left, &left);
            sync(&models.center, &center);
            sync(&models.right, &right);
        }
        if let Some(kind) = &self.popup_open
            && let Some((title, lines)) = super::status::details(c, kind)
        {
            self.popup.set_heading(title.into());
            self.popup.set_lines(ModelRc::from(Rc::new(VecModel::from(
                lines
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            ))));
        }
    }
}
