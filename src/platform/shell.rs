use super::{Event, EventSender, native};
use crate::{config::Config, layout::Rect, model::Model};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Model as _, ModelRc, VecModel};
use std::rc::Rc;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
slint::include_modules!();
#[cfg(test)]
mod bar_tests;
#[cfg(test)]
mod bar_hints_tests;
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
            ((ex & !(WS_EX_NOACTIVATE.0 as isize)) | WS_EX_TOOLWINDOW.0 as isize
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
    left: Rc<VecModel<StatusItem>>,
    center: Rc<VecModel<StatusItem>>,
    right: Rc<VecModel<StatusItem>>,
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
}
#[derive(Clone)]
pub enum MetaEntry {
    Menu(MetaMenu),
    Run(crate::command::Command),
}
pub(super) mod bar_hints;
pub(super) mod keybindings;
pub(super) mod theme_picker;
mod wallpaper;
pub struct Shell {
    pub hints: bar_hints::Hints,
    pub picker: theme_picker::Picker,
    pub editor: keybindings::Editor,
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
    pub apps: Vec<App>,
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
    wallpaper_sizes: Vec<(u32, u32)>,
    wallpaper_key: Option<crate::wallpaper::loader::Key>,
    wallpaper_loader: crate::wallpaper::loader::Loader,
    wallpaper_pending: Option<wallpaper::Pending>,
    pub wallpaper_error: Option<String>,
    /// Bar icons of the volume module: sound on, then muted.
    volume_icons: [slint::Image; 2],
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
    let text = text.to_lowercase();
    let mut chars = text.char_indices();
    let mut total = 0;
    for q in query.to_lowercase().chars() {
        let (i, _) = chars.find(|(_, c)| *c == q)?;
        total += i;
    }
    Some(total)
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
            hints: bar_hints::Hints::new()?,
            picker: theme_picker::Picker::new(tx.clone())?,
            editor: keybindings::Editor::new(tx.clone())?,
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
            wallpaper_sizes: vec![],
            wallpaper_key: None,
            wallpaper_loader: crate::wallpaper::loader::Loader::default(),
            wallpaper_pending: None,
            wallpaper_error: None,
            volume_icons: [
                slint::Image::load_from_svg_data(include_bytes!("../../ui/icons/volume.svg"))
                    .map_err(|e| e.to_string())?,
                slint::Image::load_from_svg_data(include_bytes!("../../ui/icons/volume-muted.svg"))
                    .map_err(|e| e.to_string())?,
            ],
            tx,
        })
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
        self.hints.close();
        self.picker.apply_theme(c);
        self.editor.apply_theme(c);
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
        self.refresh_wallpaper();
    }
    pub fn configure(&mut self, c: &Config, monitors: &[Rect]) -> Result<(), String> {
        self.hints.close();
        self.pending = true;
        self.picker.apply_theme(c);
        self.editor.apply_theme(c);
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
                    left: Rc::new(VecModel::default()),
                    center: Rc::new(VecModel::default()),
                    right: Rc::new(VecModel::default()),
                };
                b.set_workspaces(ModelRc::from(models.workspaces.clone()));
                b.set_left_items(ModelRc::from(models.left.clone()));
                b.set_center_items(ModelRc::from(models.center.clone()));
                b.set_right_items(ModelRc::from(models.right.clone()));
                self.models.push(models);
                self.bars.push(b);
            }
        }
        self.launcher.set_bg(color(&c.theme.background));
        self.launcher.set_fg(color(&c.theme.text));
        self.launcher.set_accent(color(&c.theme.accent));
        self.launcher.set_overlay(color(&c.theme.overlay));
        self.apps = c
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
            self.apps.push(App {
                name: label.into(),
                target: String::new(),
                shortcut: false,
                app: Some(name.into()),
            });
        }
        for env in ["APPDATA", "PROGRAMDATA"] {
            if let Some(root) = std::env::var_os(env) {
                scan(
                    &std::path::PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs"),
                    &mut self.apps,
                );
            }
        }
        for (name, target) in native::packaged_apps() {
            self.apps.push(App {
                name,
                target,
                shortcut: true,
                app: None,
            });
        }
        self.apps.sort_by_key(|a| a.name.to_lowercase());
        self.apps.dedup_by(|a, b| a.name == b.name);
        tracing::info!(count = self.apps.len(), "applications indexed");
        self.search("", c.launcher.max_results);
        Ok(())
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
            matches.sort_by_key(|(s, name, _)| (*s, (*name).clone()));
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
        let mut matches: Vec<_> = self
            .apps
            .iter()
            .filter_map(|a| score(q, &a.name).map(|s| (s, a)))
            .collect();
        matches.sort_by_key(|(s, a)| (*s, a.name.clone()));
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
        self.hints.opened || self.visible || self.picker.opened || self.editor.opened
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
            (
                "Theme ›".into(),
                MetaEntry::Run(crate::command::Command::ThemePicker),
            ),
            (
                "Wallpaper ›".into(),
                MetaEntry::Run(crate::command::Command::WallpaperPicker),
            ),
            (
                "Solid background".into(),
                MetaEntry::Run(crate::command::Command::Wallpaper(None)),
            ),
            ("Demo".into(), MetaEntry::Run(crate::command::Command::Demo)),
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
    /// Enters a submenu or returns the command to run for result `n`.
    pub fn meta_activate(&mut self, n: usize, max: usize) -> Option<crate::command::Command> {
        match self.meta_results.get(n).cloned()? {
            MetaEntry::Run(command) => Some(command),
            MetaEntry::Menu(menu) => {
                self.meta_items = match menu {
                    MetaMenu::Apps => Self::meta_apps(),
                    MetaMenu::System => Self::meta_system().unwrap_or_default(),
                };
                self.meta_menu = Some(menu);
                self.launcher.set_query("".into());
                self.launcher.set_selected(0);
                self.search("", max);
                None
            }
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
        self.launcher.show().map_err(|e| e.to_string())?;
        self.launcher_pending = Some(r);
        self.visible = true;
        Ok(())
    }
    pub fn open_hints(&mut self, c: &Config, r: Rect, monitor: usize) {
        let Some(models) = self.models.get(monitor) else {
            return;
        };
        let kinds = [&models.left, &models.center, &models.right]
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
                .filter(|n| *n == m.active || m.clients.iter().any(|w| w.workspace == *n))
                .map(i32::from)
                .collect()
        } else {
            vec![]
        };
        let title = m.focused.map(native::title).unwrap_or_default();
        // One item per configured entry, in configured order, so a module such
        // as the separator may appear several times in a section.
        let items = |modules: &[String]| {
            let mut out = Vec::new();
            for name in modules {
                let item = if let Some((label, icon)) = applets.item(name) {
                    StatusItem {
                        kind: name.clone().into(),
                        value: label.into(),
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
                        has_icon: false,
                        icon: slint::Image::default(),
                        level: i32::from(percent),
                        charging: plugged,
                        hint_id: 0,
                    }
                } else if name == "volume" {
                    let Some((_, muted)) = super::audio::volume_state() else {
                        continue;
                    };
                    StatusItem {
                        kind: "volume".into(),
                        value: "".into(),
                        has_icon: true,
                        icon: self.volume_icons[usize::from(muted)].clone(),
                        hint_id: 0,
                        level: 0,
                        charging: false,
                    }
                } else if let Some(value) = super::status::item(c, &title, name) {
                    StatusItem {
                        kind: name.clone().into(),
                        value: value.into(),
                        hint_id: 0,
                        has_icon: false,
                        icon: slint::Image::default(),
                        level: 0,
                        charging: false,
                    }
                } else {
                    continue;
                };
                out.push(item);
            }
            out
        };
        let mut left = items(&c.bar.left);
        let mut center = items(&c.bar.center);
        let mut right = items(&c.bar.right);
        let mut hint_id = 0;
        for item in left.iter_mut().chain(&mut center).chain(&mut right) {
            let kind = item.kind.as_str();
            let interactive = matches!(kind, "clock" | "battery" | "cpu" | "memory")
                || applets.is_applet(kind)
                || applets.attached(kind).is_some();
            if interactive && hint_id < crate::bar_hints::LABELS.len() as i32 {
                hint_id += 1;
                item.hint_id = hint_id;
            }
        }
        for (b, models) in self.bars.iter().zip(&self.models) {
            b.set_active(m.active as i32);
            sync(&models.workspaces, &workspaces);
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
