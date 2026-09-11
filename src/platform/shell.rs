use super::{Event, EventSender, native};
use crate::{config::Config, layout::Rect, model::Model};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use windows::Win32::UI::WindowsAndMessaging::*;
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
fn tool(w: &slint::Window, no_activate: bool) {
    unsafe {
        if id(w) == 0 {
            return;
        }
        let h = native::hwnd(id(w));
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
    pub apps: Vec<App>,
    pub results: Vec<App>,
    pub visible: bool,
    tx: EventSender,
    pub pending: bool,
    launcher_pending: Option<Rect>,
    descriptions: bool,
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
        Ok(Self {
            backgrounds: vec![],
            bars: vec![],
            launcher,
            apps: vec![],
            results: vec![],
            visible: false,
            pending: false,
            launcher_pending: None,
            descriptions: false,
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
        for r in monitors {
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
        self.apps.sort_by_key(|a| a.name.to_lowercase());
        self.apps.dedup_by(|a, b| a.name == b.name);
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
            native::focus(id(self.launcher.window()));
            self.launcher.invoke_focus_search();
            self.launcher_pending = None;
        }
        true
    }
    pub fn search(&mut self, q: &str, max: usize) {
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
        self.launcher_pending = None;
    }
    pub fn toggle(&mut self, c: &Config, r: Rect) -> Result<(), String> {
        if self.visible {
            self.dismiss();
            return Ok(());
        }
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
        let occupied = (1..=9)
            .map(|n| m.clients.iter().any(|w| w.workspace == n))
            .collect::<Vec<_>>();
        let title = m.focused.map(native::title).unwrap_or_default();
        let left = super::status::text(c, &title, &c.bar.left);
        let center = super::status::text(c, &title, &c.bar.center);
        let status = super::status::text(c, &title, &c.bar.right);
        for b in &self.bars {
            b.set_active(m.active as i32);
            b.set_occupied(ModelRc::from(Rc::new(VecModel::from(occupied.clone()))));
            b.set_workspaces_visible(c.bar.left.iter().any(|s| s == "workspaces"));
            b.set_left_text(left.clone().into());
            b.set_title_text(center.clone().into());
            b.set_status(status.clone().into());
        }
    }
}
