//! Exposé surface: a full-monitor window over the dimmed desktop with one card
//! per managed window of every workspace. Cards appear at once with title and
//! icon; thumbnails fill in as the capture worker delivers them.
use super::{ExposeCard, ExposeView, color, id, keybindings::key_from_text, tool};
use crate::{
    config::Config,
    expose::{self, Action, Entry, Model, Outcome},
    keybindings, keyboard,
    layout::Rect,
    platform::{
        Event, EventSender,
        capture::{self, Pixels},
        dpi, native,
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
    Thumbnail(isize, Option<Pixels>),
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
fn image(p: &Pixels, premultiplied: bool) -> slint::Image {
    let buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&p.rgba, p.width, p.height);
    if premultiplied {
        slint::Image::from_rgba8_premultiplied(buffer)
    } else {
        slint::Image::from_rgba8(buffer)
    }
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
    images: HashMap<isize, slint::Image>,
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
            images: HashMap::new(),
            icons: HashMap::new(),
            close_chords: vec![],
        })
    }
    pub fn apply_theme(&mut self, c: &Config) {
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_surface(color(&c.theme.surface));
        self.ui.set_overlay(color(&c.theme.overlay));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
    }
    /// `close_chords` are the configured `window close` bindings, which close
    /// the selected window from inside the exposé.
    pub fn open(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
        entries: Vec<Entry>,
        exes: HashMap<isize, String>,
        close_chords: Vec<(u32, u8)>,
    ) {
        if self.opened {
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.apply_theme(c);
        self.restore = restore;
        self.monitor = monitor;
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.ui.set_ready(false);
        self.ui.set_label_height(expose::LABEL);
        let close = close_chords
            .first()
            .and_then(|(vk, modifiers)| keyboard::format(*vk, *modifiers))
            .map(|chord| format!(" · {} close", keybindings::pretty(&chord)))
            .unwrap_or_default();
        self.ui
            .set_hint(format!("Enter or click focus{close} · middle click close · Esc").into());
        self.close_chords = close_chords;
        let (cols, slots) = expose::grid(
            entries.len(),
            dpi::logical(monitor, monitor.w),
            dpi::logical(monitor, monitor.h),
        );
        let jobs = entries
            .iter()
            .map(|e| (e.id, exes.get(&e.id).cloned().unwrap_or_default()))
            .collect();
        self.model = Model::new(entries, cols);
        self.exes = exes;
        self.images.clear();
        self.render();
        if self.ui.show().is_err() {
            return;
        }
        self.opened = true;
        self.pending_window = true;
        // Capture at the card's physical width: sharp on the card, nothing more.
        let width = slots.first().map_or(640.0, |s| s.w);
        capture::start(
            self.epoch.get(),
            jobs,
            dpi::scale(monitor, width as i32),
            self.tx.clone(),
        );
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        capture::cancel();
        self.opened = false;
        self.pending_window = false;
        let _ = self.ui.hide();
        self.rows.set_vec(vec![]);
        self.images.clear();
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
        self.images.remove(&id);
        self.render();
    }
    fn render(&mut self) {
        let (w, h) = (
            dpi::logical(self.monitor, self.monitor.w),
            dpi::logical(self.monitor, self.monitor.h),
        );
        let (cols, slots) = expose::grid(self.model.shown.len(), w, h);
        self.model.cols = cols;
        let rows: Vec<ExposeCard> = self
            .model
            .shown
            .iter()
            .zip(slots)
            .map(|(index, slot)| {
                let e = &self.model.entries[*index];
                let icon = self.exes.get(&e.id).and_then(|exe| self.icons.get(exe));
                let image = self.images.get(&e.id);
                ExposeCard {
                    title: e.title.clone().into(),
                    app: e.app.clone().into(),
                    workspace: e.workspace.to_string().into(),
                    image: image.cloned().unwrap_or_default(),
                    has_image: image.is_some(),
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
                self.icons.insert(exe, image(&pixels, true));
                Outcome::None
            }
            Input::Thumbnail(id, pixels) => {
                if let Some(pixels) = pixels {
                    self.images.insert(id, image(&pixels, false));
                }
                Outcome::None
            }
        };
        if outcome == Outcome::None {
            self.render();
        }
        outcome
    }
}
