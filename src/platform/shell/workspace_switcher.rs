//! Live miniature desktops. Slint and DWM consume the same interpolated geometry.
#[cfg(test)]
#[path = "workspace_switcher_tests.rs"]
mod tests;
use super::{WorkspaceCard, WorkspacePlaceholder, WorkspaceSwitcherView, color, id, tool};
use crate::{
    config::Config,
    layout::Rect,
    platform::{Event, EventSender, dpi, native, thumbnails::Thumbnails},
    workspace_switcher::{BoxRect, Model, thumbnail},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::Cell, rc::Rc, time::Instant};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

#[derive(Clone, Debug)]
pub enum Input {
    Key(String),
    Select(u8),
    Dismiss,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    None,
    Close,
    Activate(u8),
}

pub struct Window {
    pub id: isize,
    pub frame: Rect,
    pub source: Rect,
    pub live: bool,
    pub minimized: bool,
    pub title: String,
}
pub struct Desktop {
    pub monitor: Rect,
    pub wallpaper: slint::Image,
    /// Bottom to top, matching the native window stacking order.
    pub windows: Vec<Window>,
}
pub struct Scene {
    pub active: u8,
    pub desktops: Vec<Desktop>,
    pub backdrop: slint::Image,
}
pub struct Switcher {
    ui: WorkspaceSwitcherView,
    rows: Rc<VecModel<WorkspaceCard>>,
    placeholders: Rc<VecModel<WorkspacePlaceholder>>,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending: bool,
    restore: Option<isize>,
    monitor: Rect,
    model: Model,
    scene: Option<Scene>,
    thumbnails: Option<Thumbnails>,
    last: Instant,
    reveal: f32,
    closing: Outcome,
}
impl Switcher {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = WorkspaceSwitcherView::new().map_err(|e| e.to_string())?;
        let rows = Rc::new(VecModel::default());
        let placeholders = Rc::new(VecModel::default());
        ui.set_cards(ModelRc::from(rows.clone()));
        ui.set_placeholders(ModelRc::from(placeholders.clone()));
        let epoch = Rc::new(Cell::new(0));
        let send = |tx: &EventSender| {
            let (tx, epoch) = (tx.clone(), epoch.clone());
            move |input| {
                let _ = tx.send(Event::WorkspaceSwitcher(epoch.get(), input));
            }
        };
        let f = send(&tx);
        ui.on_key(move |text| f(Input::Key(text.into())));
        let f = send(&tx);
        ui.on_select(move |n| f(Input::Select(n.clamp(1, 9) as u8)));
        let f = send(&tx);
        ui.on_dismiss(move || f(Input::Dismiss));
        let f = send(&tx);
        ui.window().on_close_requested(move || {
            f(Input::Dismiss);
            slint::CloseRequestResponse::KeepWindowShown
        });
        Ok(Self {
            ui,
            rows,
            placeholders,
            epoch,
            opened: false,
            pending: false,
            restore: None,
            monitor: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            model: Model::new(1),
            scene: None,
            thumbnails: None,
            last: Instant::now(),
            reveal: 0.0,
            closing: Outcome::None,
        })
    }
    pub fn lost_focus(&mut self, foreground: isize) {
        if self.opened && !self.pending && foreground != 0 && foreground != id(self.ui.window()) {
            // An external activation wins; do not restore the old focus over it.
            self.close();
        }
    }
    pub fn selected(&self) -> Option<u8> {
        self.opened.then_some(self.model.selected)
    }
    pub fn apply_theme(&self, c: &Config) {
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_accent(color(&c.theme.accent));
    }
    pub fn open(&mut self, c: &Config, monitor: Rect, restore: Option<isize>, scene: Scene) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.apply_theme(c);
        self.monitor = monitor;
        self.restore = restore;
        self.model = Model::new(scene.active);
        self.ui.set_backdrop(scene.backdrop.clone());
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.ui.set_ready(false);
        self.scene = Some(scene);
        self.reveal = 0.0;
        self.closing = Outcome::None;
        self.render();
        super::prepare(self.ui.window(), monitor, false);
        if self.ui.show().is_err() {
            self.scene = None;
            return;
        }
        self.opened = true;
        self.pending = true;
    }
    /// Layout changes are infrequent: replace metadata, not the selection or
    /// animation. Re-register to discard obsolete/reused native handles.
    pub fn update(&mut self, scene: Scene) {
        if !self.opened {
            return;
        }
        self.scene = Some(scene);
        self.thumbnails = (!self.pending).then(|| Thumbnails::new(id(self.ui.window())));
        self.render();
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.opened = false;
        self.pending = false;
        // DWM registrations must be released before hiding the destination.
        self.thumbnails = None;
        let _ = self.ui.hide();
        self.rows.set_vec(vec![]);
        self.placeholders.set_vec(vec![]);
        self.scene = None;
        self.restore.take()
    }
    /// Called by the shell's existing 10ms poll, never a separate animation queue.
    pub fn poll(&mut self) -> Outcome {
        if !self.opened {
            return Outcome::None;
        }
        if self.pending {
            if id(self.ui.window()) == 0 {
                return Outcome::None;
            }
            tool(self.ui.window(), false);
            native::position(id(self.ui.window()), self.monitor, Some(HWND_TOPMOST));
            native::focus(id(self.ui.window()), false);
            self.ui.set_ready(true);
            self.ui.invoke_focus_view();
            self.thumbnails = Some(Thumbnails::new(id(self.ui.window())));
            self.pending = false;
            self.last = Instant::now();
            self.render();
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        let old = self.reveal;
        if self.closing == Outcome::None {
            self.reveal = (self.reveal + dt / 0.16).min(1.0);
        } else {
            self.reveal = (self.reveal - dt / 0.12).max(0.0);
            if self.reveal == 0.0 {
                return self.closing;
            }
        }
        if self.model.advance(dt) || old != self.reveal {
            self.render();
        }
        Outcome::None
    }
    pub fn input(&mut self, epoch: u64, input: Input) {
        if !self.opened || epoch != self.epoch.get() || self.closing != Outcome::None {
            return;
        }
        match input {
            Input::Dismiss => self.closing = Outcome::Close,
            Input::Select(n) => self.model.select(n),
            Input::Key(text) => {
                use slint::platform::Key as K;
                let Some(c) = text.chars().next() else {
                    return;
                };
                if c == char::from(K::Escape) {
                    self.closing = Outcome::Close;
                } else if c == char::from(K::Return) {
                    self.closing = Outcome::Activate(self.model.selected);
                } else if [char::from(K::LeftArrow), char::from(K::UpArrow), 'h', 'k'].contains(&c)
                {
                    self.model.step(-1);
                } else if [
                    char::from(K::RightArrow),
                    char::from(K::DownArrow),
                    'j',
                    'l',
                ]
                .contains(&c)
                {
                    self.model.step(1);
                } else if ('1'..='9').contains(&c) {
                    self.model.select(c as u8 - b'0');
                }
            }
        }
        self.render();
    }
    pub fn remove(&mut self, id: isize) {
        if let Some(scene) = &mut self.scene {
            for desktop in &mut scene.desktops {
                desktop.windows.retain(|w| w.id != id);
            }
            if let Some(thumbnails) = &mut self.thumbnails {
                thumbnails.remove(id);
            }
            self.render();
        }
    }
    fn render(&mut self) {
        let Some(scene) = &self.scene else {
            return;
        };
        let width = dpi::logical(self.monitor, self.monitor.w);
        let height = dpi::logical(self.monitor, self.monitor.h);
        let viewport = BoxRect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        };
        let physical = |r: BoxRect| {
            let scale = self.monitor.w as f32 / width;
            let x = (r.x * scale).round() as i32;
            let y = (r.y * scale).round() as i32;
            Rect {
                x,
                y,
                w: ((r.x + r.w) * scale).round() as i32 - x,
                h: ((r.y + r.h) * scale).round() as i32 - y,
            }
        };
        let mut cards = Vec::with_capacity(9);
        let mut placeholders = Vec::new();
        let mut shown = Vec::new();
        for (index, desktop) in scene.desktops.iter().enumerate() {
            let n = index as u8 + 1;
            let (card, opacity) = self.model.card(n, width, height);
            let inner = BoxRect {
                x: card.x + 2.0,
                y: card.y + 2.0,
                w: card.w - 4.0,
                h: card.h - 4.0,
            }
            .fit(desktop.monitor);
            cards.push(WorkspaceCard {
                x: card.x,
                y: card.y,
                width: card.w,
                height: card.h,
                desktop_x: inner.x,
                desktop_y: inner.y,
                desktop_width: inner.w,
                desktop_height: inner.h,
                wallpaper: desktop.wallpaper.clone(),
                strength: opacity,
                label: format!("{}{}", if n == scene.active { "• " } else { "" }, n).into(),
                detail: match desktop.windows.len() {
                    0 => "Empty".into(),
                    1 => desktop.windows[0].title.clone().into(),
                    count => format!("{count} windows").into(),
                },
            });
            // Traverse top to bottom and explicitly subtract occluders. DWM
            // thumbnails always draw over Slint, and have no supported Z-order
            // API. Disjoint fragments make that ordering irrelevant.
            let mut covers = Vec::new();
            for window in desktop.windows.iter().rev().filter(|w| !w.minimized) {
                let Some((dest, _)) = thumbnail(
                    window.frame,
                    window.source,
                    desktop.monitor,
                    inner,
                    viewport,
                ) else {
                    continue;
                };
                if !window.live {
                    // Slint can stack fallback rectangles normally. Insert in
                    // bottom-to-top paint order, without fragmenting their text.
                    placeholders.insert(
                        0,
                        WorkspacePlaceholder {
                            x: dest.x,
                            y: dest.y,
                            width: dest.w,
                            height: dest.h,
                            title: window.title.clone().into(),
                        },
                    );
                    covers.push(dest);
                    continue;
                }
                let mut pieces = vec![dest];
                for cover in &covers {
                    pieces = pieces
                        .into_iter()
                        .flat_map(|r| r.subtract(*cover))
                        .collect();
                }
                covers.push(dest);
                for (part, piece) in pieces.into_iter().enumerate() {
                    let Some((dest, region)) =
                        thumbnail(window.frame, window.source, desktop.monitor, inner, piece)
                    else {
                        continue;
                    };
                    let pixels = physical(dest);
                    if pixels.w <= 0 || pixels.h <= 0 {
                        continue;
                    }
                    let live = window.live
                        && self.thumbnails.as_mut().is_some_and(|thumbnails| {
                            thumbnails.place_part(
                                window.id,
                                part,
                                pixels,
                                Some(region),
                                (255.0 * opacity * self.reveal).round() as u8,
                            )
                        });
                    if live {
                        shown.push((window.id, part));
                    } else {
                        placeholders.push(WorkspacePlaceholder {
                            x: dest.x,
                            y: dest.y,
                            width: dest.w,
                            height: dest.h,
                            title: window.title.clone().into(),
                        });
                    }
                }
            }
        }
        if let Some(thumbnails) = &mut self.thumbnails {
            thumbnails.retain_parts(&shown);
        }
        super::sync(&self.rows, &cards);
        super::sync(&self.placeholders, &placeholders);
        self.ui.set_selected(self.model.selected as i32);
        self.ui.set_reveal(self.reveal);
    }
}
