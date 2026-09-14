use super::{Event, EventSender, native};
use crate::{config::Config, layout::Rect, model::Model};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
slint::include_modules!();
fn color(s: &str) -> slint::Color {
    let c = u32::from_str_radix(&s[1..], 16).unwrap_or_default();
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
fn id(w: &slint::Window) -> isize {
    match w.window_handle().window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}
const ORIGINAL_PROC: &str = "WinarchyOriginalProc";
/// Windows paints a classic caption over the top of a winit window each time
/// it is activated, even without WS_CAPTION, and leaves it there until the
/// next redraw. Answering the non-client messages ourselves prevents that.
unsafe extern "system" fn surface_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_NCACTIVATE {
        return LRESULT(1);
    }
    if m == WM_NCPAINT {
        return LRESULT(0);
    }
    unsafe {
        let key = native::wide(ORIGINAL_PROC);
        let original = GetPropW(h, windows::core::PCWSTR(key.as_ptr())).0 as isize;
        if original == 0 {
            return DefWindowProcW(h, m, w, l);
        }
        let original: WNDPROC = std::mem::transmute::<isize, WNDPROC>(original);
        CallWindowProcW(original, h, m, w, l)
    }
}
fn tool(w: &slint::Window, no_activate: bool) {
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
            (ex | WS_EX_TOOLWINDOW.0 as isize
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
}
pub struct Shell {
    pub backgrounds: Vec<Background>,
    pub bars: Vec<Bar>,
    pub launcher: Launcher,
    popup: Popup,
    /// Kind of the module whose popup is open.
    pub popup_open: Option<String>,
    popup_pending: Option<Rect>,
    pub apps: Vec<App>,
    pub results: Vec<App>,
    pub visible: bool,
    tx: EventSender,
    pub pending: bool,
    launcher_pending: Option<Rect>,
    descriptions: bool,
    /// The launcher surface shows the session menu instead of applications.
    pub meta: bool,
    meta_items: Vec<(String, crate::command::Command)>,
    pub meta_results: Vec<crate::command::Command>,
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
        let popup = Popup::new().map_err(|e| e.to_string())?;
        Ok(Self {
            backgrounds: vec![],
            bars: vec![],
            launcher,
            popup,
            popup_open: None,
            popup_pending: None,
            apps: vec![],
            results: vec![],
            visible: false,
            pending: false,
            launcher_pending: None,
            descriptions: false,
            meta: false,
            meta_items: vec![],
            meta_results: vec![],
            tx,
        })
    }
    pub fn configure(&mut self, c: &Config, monitors: &[Rect]) -> Result<(), String> {
        self.pending = true;
        self.descriptions = c.launcher.show_descriptions;
        for b in self.bars.drain(..) {
            let _ = b.hide();
        }
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
                b.set_surface_width(super::dpi::logical(*r, r.w));
                b.set_surface_height(c.bar.height as f32);
                b.show().map_err(|e| e.to_string())?;
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
            })
            .collect();
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
            tool(self.popup.window(), true);
            native::position(id(self.popup.window()), r, Some(HWND_TOPMOST));
            self.popup_pending = None;
        }
        true
    }
    /// Popups, bars and backgrounds are the shell's own windows.
    pub fn owns(&self, id_: isize) -> bool {
        id(self.popup.window()) == id_
            || self.bars.iter().any(|b| id(b.window()) == id_)
            || self.backgrounds.iter().any(|b| id(b.window()) == id_)
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
    pub fn close_popup(&mut self) {
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
        use crate::command::Command;
        let shutdown = super::security::os_executable("shutdown.exe", true)?;
        let rundll = super::security::os_executable("rundll32.exe", true)?;
        let run = |line: String| Command::LaunchTarget {
            target: line,
            shortcut: false,
        };
        self.meta_items = vec![
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
            ("Quit Winarchy".into(), Command::Quit),
        ];
        self.meta = true;
        self.open(c, r)
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
    pub fn refresh(&self, m: &Model, c: &Config) {
        let workspaces: Vec<i32> = if c.bar.left.iter().any(|s| s == "workspaces") {
            (1..=9u8)
                .filter(|n| *n == m.active || m.clients.iter().any(|w| w.workspace == *n))
                .map(i32::from)
                .collect()
        } else {
            vec![]
        };
        let title = m.focused.map(native::title).unwrap_or_default();
        let items = |modules: &[String]| {
            super::status::items(c, &title, modules)
                .into_iter()
                .map(|(kind, value)| StatusItem {
                    kind: kind.into(),
                    value: value.into(),
                })
                .collect::<Vec<_>>()
        };
        let left = items(&c.bar.left);
        let center = items(&c.bar.center);
        let right = items(&c.bar.right);
        for b in &self.bars {
            b.set_active(m.active as i32);
            b.set_workspaces(ModelRc::from(Rc::new(VecModel::from(workspaces.clone()))));
            b.set_left_items(ModelRc::from(Rc::new(VecModel::from(left.clone()))));
            b.set_center_items(ModelRc::from(Rc::new(VecModel::from(center.clone()))));
            b.set_right_items(ModelRc::from(Rc::new(VecModel::from(right.clone()))));
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
