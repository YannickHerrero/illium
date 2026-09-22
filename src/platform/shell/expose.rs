//! Exposé surface: a full-monitor window over the blurred wallpaper with one
//! card per managed window of every workspace. The DWM composes each window
//! live into its card; icons arrive from a worker thread.
use super::{ExposeCard, ExposeView, color, id, keybindings::key_from_text, tool};
use crate::{
    config::Config,
    expose::{self, Action, Entry, Model, Outcome},
    keybindings, keyboard,
    layout::Rect,
    platform::{
        Event, EventSender, dpi,
        icons::{self, Pixels},
        native,
        thumbnails::Thumbnails,
    },
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::Cell, collections::HashMap, rc::Rc};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

#[derive(Clone, Debug)]
pub enum Input {
    /// Key text and the Ctrl, Shift, Alt and Meta modifiers held.
    Key(String, bool, bool, bool, bool),
    Activate(i32),
    CloseWindow(i32),
    Hover(i32),
    Dismiss,
    Icon(String, Pixels),
}
fn action(
    text: &str,
    control: bool,
    shift: bool,
    alt: bool,
    meta: bool,
    close: &[(u32, u8)],
) -> Option<Action> {
    use slint::platform::Key as K;
    let c = text.chars().next()?;
    let special = |k: K| char::from(k) == c;
    let modifiers = [
        (control, keyboard::CTRL),
        (shift, keyboard::SHIFT),
        (alt, keyboard::ALT),
        (meta, keyboard::SUPER),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(0, |mask, (_, bit)| mask | bit);
    if close.contains(&(key_from_text(text), modifiers)) {
        return Some(Action::CloseWindow);
    }
    Some(if special(K::Escape) {
        Action::Escape
    } else if special(K::Return) {
        Action::Confirm
    } else if special(K::Backspace) && !alt && !meta {
        if control {
            Action::Clear
        } else {
            Action::Backspace
        }
    } else if control && !shift && !alt && !meta && c.eq_ignore_ascii_case(&'u') {
        Action::Clear
    } else if special(K::LeftArrow) || (special(K::Tab) && shift) || special(K::Backtab) {
        Action::Previous
    } else if special(K::RightArrow) || special(K::Tab) {
        Action::Next
    } else if special(K::UpArrow) {
        Action::Up
    } else if special(K::DownArrow) {
        Action::Down
    } else if !control
        && !alt
        && !meta
        && !c.is_control()
        && !('\u{e000}'..='\u{f8ff}').contains(&c)
    {
        Action::Text(text.into())
    } else {
        return None;
    })
}
fn image(p: &Pixels) -> slint::Image {
    slint::Image::from_rgba8_premultiplied(
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&p.rgba, p.width, p.height),
    )
}
/// Selected card border, kept clear of the DWM thumbnail drawn inside it.
const BORDER: f32 = 2.0;
const MINIMIZED_OPACITY: u8 = 140;
/// Everything the manager gathers for one opening.
pub struct Scene {
    pub entries: Vec<Entry>,
    /// Executable path per window, for the icon lookup.
    pub exes: HashMap<isize, String>,
    /// Visible frame of each window relative to its own top-left corner, so
    /// the thumbnail leaves out the invisible resize borders.
    pub sources: HashMap<isize, Rect>,
    /// The configured `window close` bindings, which close the selected
    /// window from inside the exposé.
    pub close_chords: Vec<(u32, u8)>,
    /// Blurred wallpaper of the monitor, empty on a solid background.
    pub backdrop: slint::Image,
}
pub struct Expose {
    ui: ExposeView,
    rows: Rc<VecModel<ExposeCard>>,
    epoch: Rc<Cell<u64>>,
    tx: EventSender,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    monitor: Rect,
    model: Model,
    /// Executable path per window, for the icon lookup.
    exes: HashMap<isize, String>,
    sources: HashMap<isize, Rect>,
    /// Live previews, registered once the native window exists.
    thumbnails: Option<Thumbnails>,
    /// Icons by executable path, kept across openings.
    icons: HashMap<String, slint::Image>,
    close_chords: Vec<(u32, u8)>,
}
impl Expose {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = ExposeView::new().map_err(|e| e.to_string())?;
        let rows = Rc::new(VecModel::default());
        ui.set_cards(ModelRc::from(rows.clone()));
        let epoch = Rc::new(Cell::new(0));
        let send = |tx: &EventSender, epoch: &Rc<Cell<u64>>| {
            let (t, e) = (tx.clone(), epoch.clone());
            move |input: Input| {
                let _ = t.send(Event::Expose(e.get(), input));
            }
        };
        let f = send(&tx, &epoch);
        ui.on_key(move |text, ctrl, shift, alt, meta| {
            f(Input::Key(text.into(), ctrl, shift, alt, meta));
        });
        let f = send(&tx, &epoch);
        ui.on_activate(move |n| f(Input::Activate(n)));
        let f = send(&tx, &epoch);
        ui.on_close_window(move |n| f(Input::CloseWindow(n)));
        let f = send(&tx, &epoch);
        ui.on_hover(move |n| f(Input::Hover(n)));
        let f = send(&tx, &epoch);
        ui.on_dismiss(move || f(Input::Dismiss));
        let f = send(&tx, &epoch);
        ui.window().on_close_requested(move || {
            f(Input::Dismiss);
            slint::CloseRequestResponse::KeepWindowShown
        });
        Ok(Self {
            ui,
            rows,
            epoch,
            tx,
            opened: false,
            pending_window: false,
            restore: None,
            monitor: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            model: Model::default(),
            exes: HashMap::new(),
            sources: HashMap::new(),
            thumbnails: None,
            icons: HashMap::new(),
            close_chords: vec![],
        })
    }
    pub fn prewarm(&self) { if !self.opened { super::prewarm(self.ui.window()); } }
    pub fn apply_theme(&mut self, c: &Config) {
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_surface(color(&c.theme.surface));
        self.ui.set_overlay(color(&c.theme.overlay));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
    }
    pub fn open(&mut self, c: &Config, monitor: Rect, restore: Option<isize>, scene: Scene) {
        if self.opened {
            return;
        }
        let Scene {
            entries,
            exes,
            sources,
            close_chords,
            backdrop,
        } = scene;
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.apply_theme(c);
        self.restore = restore;
        self.monitor = monitor;
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.ui.set_ready(false);
        self.ui.set_label_height(expose::LABEL);
        let has_backdrop = backdrop.size().width > 0;
        self.ui.set_backdrop(backdrop);
        self.ui.set_has_backdrop(has_backdrop);
        let close = close_chords
            .first()
            .and_then(|(vk, modifiers)| keyboard::format(*vk, *modifiers))
            .map(|chord| format!("   {} close window", keybindings::pretty(&chord)))
            .unwrap_or_default();
        self.ui.set_hint(
            format!("← ↑ ↓ → navigate   Enter open{close}   middle click close window   Esc close")
                .into(),
        );
        self.close_chords = close_chords;
        let missing: Vec<String> = exes
            .values()
            .filter(|exe| !self.icons.contains_key(*exe))
            .cloned()
            .collect();
        self.model = Model::new(entries);
        self.exes = exes;
        self.sources = sources;
        self.render();
        super::prepare(self.ui.window(), monitor, false);
        if self.ui.show().is_err() {
            return;
        }
        self.opened = true;
        self.pending_window = true;
        if !missing.is_empty() {
            icons::start(self.epoch.get(), missing, self.tx.clone());
        }
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        icons::cancel();
        self.opened = false;
        self.pending_window = false;
        // Unregister before the window hides: DWM keeps drawing otherwise.
        self.thumbnails = None;
        let _ = self.ui.hide();
        self.rows.set_vec(vec![]);
        self.model = Model::default();
        self.restore.take()
    }
    pub fn arrange(&mut self) {
        if !self.opened || !self.pending_window || id(self.ui.window()) == 0 {
            return;
        }
        tool(self.ui.window(), false);
        native::position(id(self.ui.window()), self.monitor, Some(HWND_TOPMOST));
        native::focus(id(self.ui.window()), false);
        self.ui.set_ready(true);
        self.ui.invoke_focus_view();
        self.pending_window = false;
        if self.thumbnails.is_none() {
            self.thumbnails = Some(Thumbnails::new(id(self.ui.window())));
        }
        self.place_thumbnails();
    }
    pub fn display_changed(&mut self, monitor: Rect) {
        if !self.opened {
            return;
        }
        self.monitor = monitor;
        self.pending_window = true;
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.render();
    }
    /// A window closed from the exposé or destroyed while it is open.
    pub fn remove(&mut self, id: isize) {
        if !self.opened {
            return;
        }
        self.model.remove(id);
        if let Some(thumbnails) = &mut self.thumbnails {
            thumbnails.remove(id);
        }
        self.render();
    }
    /// Destination rectangles in physical client pixels of the surface, inset
    /// by the border Slint draws around each card.
    fn place_thumbnails(&mut self) {
        let Some(thumbnails) = &mut self.thumbnails else {
            return;
        };
        let monitor = self.monitor;
        let physical = |v: f32| dpi::scale(monitor, v.round() as i32);
        let mut shown = Vec::with_capacity(self.model.shown.len());
        for (index, slot) in self.model.shown.iter().zip(&self.model.slots) {
            let e = &self.model.entries[*index];
            if !e.live {
                continue;
            }
            shown.push(e.id);
            let dest = Rect {
                x: physical(slot.x + BORDER),
                y: physical(slot.y + BORDER),
                w: physical(slot.w - 2.0 * BORDER).max(1),
                h: physical(slot.h - 2.0 * BORDER).max(1),
            };
            // A minimized window's rectangle is a placeholder: show it whole.
            let region = (!e.minimized)
                .then(|| self.sources.get(&e.id).copied())
                .flatten();
            let opacity = if e.minimized { MINIMIZED_OPACITY } else { 255 };
            thumbnails.place(e.id, dest, region, opacity);
        }
        thumbnails.hide_others(&shown);
    }
    fn render(&mut self) {
        let (w, h) = (
            dpi::logical(self.monitor, self.monitor.w),
            dpi::logical(self.monitor, self.monitor.h),
        );
        self.model.arrange(w, h);
        let rows: Vec<ExposeCard> = self
            .model
            .shown
            .iter()
            .zip(self.model.slots.iter().copied())
            .map(|(index, slot)| {
                let e = &self.model.entries[*index];
                let icon = self.exes.get(&e.id).and_then(|exe| self.icons.get(exe));
                ExposeCard {
                    title: e.title.clone().into(),
                    app: e.app.clone().into(),
                    workspace: e.workspace.to_string().into(),
                    live: e.live,
                    icon: icon.cloned().unwrap_or_default(),
                    has_icon: icon.is_some(),
                    focused: e.focused,
                    minimized: e.minimized,
                    x: slot.x,
                    y: slot.y,
                    width: slot.w,
                    height: slot.h,
                }
            })
            .collect();
        super::sync(&self.rows, &rows);
        self.ui.set_selected(self.model.selected as i32);
        self.ui.set_query(self.model.query.clone().into());
        self.place_thumbnails();
        let n = self.model.shown.len();
        self.ui.set_count(
            match n {
                1 => "1 window".to_owned(),
                n => format!("{n} windows"),
            }
            .into(),
        );
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        let outcome = match input {
            Input::Key(text, ctrl, shift, alt, meta) => {
                match action(&text, ctrl, shift, alt, meta, &self.close_chords) {
                    Some(action) => self.model.action(action),
                    None => Outcome::None,
                }
            }
            Input::Activate(n) => {
                self.model.select(n.max(0) as usize);
                self.model.action(Action::Confirm)
            }
            Input::CloseWindow(n) => {
                self.model.select(n.max(0) as usize);
                self.model.action(Action::CloseWindow)
            }
            Input::Hover(n) => {
                let n = n.max(0) as usize;
                if n == self.model.selected {
                    return Outcome::None;
                }
                self.model.select(n);
                Outcome::None
            }
            Input::Dismiss => Outcome::Close,
            Input::Icon(exe, pixels) => {
                self.icons.insert(exe, image(&pixels));
                Outcome::None
            }
        };
        if outcome == Outcome::None {
            self.render();
        }
        outcome
    }
}
