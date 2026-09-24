//! Space picker surface: a full-monitor window over the blurred wallpaper
//! with the named spaces in a centered card.
use super::{SpacePickerView, SpaceRow, color, id, tool};
use crate::{
    config::Config,
    layout::Rect,
    platform::{Event, EventSender, dpi, native},
    space_picker::{Action, Mode, Model, Outcome, Row},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::Cell, rc::Rc};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

#[derive(Clone, Debug)]
pub enum Input {
    /// Key text and the Ctrl, Shift, Alt and Meta modifiers held.
    Key(String, bool, bool, bool, bool),
    Activate(i32),
    Hover(i32),
    Dismiss,
}
fn action(text: &str, control: bool, shift: bool, alt: bool, meta: bool) -> Option<Action> {
    use slint::platform::Key as K;
    let c = text.chars().next()?;
    let special = |k: K| char::from(k) == c;
    Some(if special(K::Escape) {
        Action::Escape
    } else if special(K::Return) {
        Action::Confirm
    } else if special(K::Backspace) {
        Action::Backspace
    } else if special(K::UpArrow) || (special(K::Tab) && shift) || special(K::Backtab) {
        Action::Previous
    } else if special(K::DownArrow) || special(K::Tab) {
        Action::Next
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
const HINT: &str = "↑ ↓ select   Enter switch   N new   E rename   D delete   Esc close";
pub struct SpacePicker {
    ui: SpacePickerView,
    rows: Rc<VecModel<SpaceRow>>,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    monitor: Rect,
    model: Model,
}
impl SpacePicker {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = SpacePickerView::new().map_err(|e| e.to_string())?;
        let rows = Rc::new(VecModel::default());
        ui.set_rows(ModelRc::from(rows.clone()));
        let epoch = Rc::new(Cell::new(0));
        let send = |tx: &EventSender, epoch: &Rc<Cell<u64>>| {
            let (t, e) = (tx.clone(), epoch.clone());
            move |input: Input| {
                let _ = t.send(Event::SpacePicker(e.get(), input));
            }
        };
        let f = send(&tx, &epoch);
        ui.on_key(move |text, ctrl, shift, alt, meta| {
            f(Input::Key(text.into(), ctrl, shift, alt, meta));
        });
        let f = send(&tx, &epoch);
        ui.on_activate(move |n| f(Input::Activate(n)));
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
        })
    }
    pub fn prewarm(&self) {
        if !self.opened {
            super::prewarm(&self.ui);
        }
    }
    pub fn apply_theme(&mut self, c: &Config) {
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_surface(color(&c.theme.surface));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
    }
    pub fn open(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
        backdrop: slint::Image,
        rows: Vec<Row>,
        selected: usize,
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
        let has_backdrop = backdrop.size().width > 0;
        self.ui.set_backdrop(backdrop);
        self.ui.set_has_backdrop(has_backdrop);
        self.model = Model::new(rows, selected);
        self.render();
        super::prepare(self.ui.window(), monitor, false);
        if self.ui.show().is_err() {
            return;
        }
        self.opened = true;
        self.pending_window = true;
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.opened = false;
        self.pending_window = false;
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
    }
    pub fn display_changed(&mut self, monitor: Rect) {
        if !self.opened {
            return;
        }
        self.monitor = monitor;
        self.pending_window = true;
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
    }
    /// The picker's own chord pressed again while it is open.
    pub fn next(&mut self) {
        if self.opened {
            self.model.action(Action::Next);
            self.render();
        }
    }
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.model.set_rows(rows);
        self.render();
    }
    pub fn fail(&mut self, error: String) {
        self.model.error = Some(error);
        self.render();
    }
    fn render(&mut self) {
        let rows: Vec<SpaceRow> = self
            .model
            .rows
            .iter()
            .map(|r| SpaceRow {
                name: r.name.clone().into(),
                apps: r.apps.join(", ").into(),
                current: r.current,
            })
            .collect();
        super::sync(&self.rows, &rows);
        self.ui.set_selected(self.model.selected as i32);
        let selected = self
            .model
            .rows
            .get(self.model.selected)
            .map_or("", |r| r.name.as_str());
        let prompt = match &self.model.mode {
            Mode::Browse => String::new(),
            Mode::Create(name) => format!("New space: {name}▏"),
            Mode::Rename(name) => format!("Rename {selected}: {name}▏"),
            Mode::Delete => format!("Press D again to delete {selected}; its windows are kept"),
        };
        self.ui.set_prompt(prompt.into());
        self.ui
            .set_error(self.model.error.clone().unwrap_or_default().into());
        let hint = match self.model.mode {
            Mode::Browse => HINT,
            Mode::Create(_) | Mode::Rename(_) => "Enter confirm   Esc cancel",
            Mode::Delete => "Any other key cancels",
        };
        self.ui.set_hint(hint.into());
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        let outcome = match input {
            Input::Key(text, ctrl, shift, alt, meta) => match action(&text, ctrl, shift, alt, meta)
            {
                Some(action) => self.model.action(action),
                None => Outcome::None,
            },
            Input::Activate(n) => {
                self.model.mode = Mode::Browse;
                self.model.select(n.max(0) as usize);
                self.model.action(Action::Confirm)
            }
            Input::Hover(n) => {
                let n = n.max(0) as usize;
                if n == self.model.selected || self.model.mode != Mode::Browse {
                    return Outcome::None;
                }
                self.model.select(n);
                Outcome::None
            }
            Input::Dismiss => Outcome::Close,
        };
        self.render();
        outcome
    }
}
