//! Keybindings editor window. Chords are captured by the keyboard hook, not by
//! Slint, so the recorded key and modifiers are exactly what the hook matches.
use super::{KeyRow, KeybindingsEditor, color, id, tool};
use crate::{
    config::Config,
    keybindings::{self, Row},
    keyboard,
    layout::Rect,
    platform::{Event, EventSender, dpi, native},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::Cell, path::PathBuf, rc::Rc};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

#[derive(Clone, Debug)]
pub enum Input {
    Search(String),
    Change(i32),
    Reset(i32),
    Dismiss,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Apply {
    /// Chords released: the row's previous one and a replaced conflict.
    pub remove: Vec<String>,
    pub chord: String,
    pub command: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    None,
    Close,
    Apply(Apply),
}
struct Capture {
    row: usize,
    chord: Option<(u32, u8)>,
    conflict: Option<usize>,
    reset: bool,
    error: Option<String>,
}
pub struct Editor {
    ui: KeybindingsEditor,
    model: Rc<VecModel<KeyRow>>,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    monitor: Rect,
    home: PathBuf,
    rows: Vec<Row>,
    shown: Vec<usize>,
    capture: Option<Capture>,
    /// Modifiers currently held while capturing, for the chips.
    held: u8,
}
fn modifier_names(mask: u8) -> Vec<String> {
    [
        (keyboard::CTRL, "Ctrl"),
        (keyboard::ALT, "Alt"),
        (keyboard::SHIFT, "Shift"),
        (keyboard::SUPER, "Super"),
    ]
    .into_iter()
    .filter(|(bit, _)| mask & bit != 0)
    .map(|(_, name)| name.to_owned())
    .collect()
}
impl Editor {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = KeybindingsEditor::new().map_err(|e| e.to_string())?;
        let model = Rc::new(VecModel::default());
        ui.set_rows(ModelRc::from(model.clone()));
        let epoch = Rc::new(Cell::new(0));
        let send = |tx: &EventSender, epoch: &Rc<Cell<u64>>| {
            let (t, e) = (tx.clone(), epoch.clone());
            move |input: Input| {
                let _ = t.send(Event::Keybindings(e.get(), input));
            }
        };
        let f = send(&tx, &epoch);
        ui.on_search(move |q| f(Input::Search(q.into())));
        let f = send(&tx, &epoch);
        ui.on_change(move |n| f(Input::Change(n)));
        let f = send(&tx, &epoch);
        ui.on_reset(move |n| f(Input::Reset(n)));
        let f = send(&tx, &epoch);
        ui.on_dismiss(move || f(Input::Dismiss));
        let f = send(&tx, &epoch);
        ui.window().on_close_requested(move || {
            f(Input::Dismiss);
            slint::CloseRequestResponse::KeepWindowShown
        });
        Ok(Self {
            ui,
            model,
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
            home: PathBuf::new(),
            rows: vec![],
            shown: vec![],
            capture: None,
            held: 0,
        })
    }
    pub fn apply_theme(&mut self, c: &Config) {
        self.home = c.home.clone();
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_surface(color(&c.theme.surface));
        self.ui.set_overlay(color(&c.theme.overlay));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
        self.ui.set_warning(color(&c.theme.yellow));
    }
    pub fn open(&mut self, c: &Config, monitor: Rect, restore: Option<isize>) {
        if self.opened {
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.apply_theme(c);
        self.restore = restore;
        self.monitor = monitor;
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.ui.set_query("".into());
        self.ui.set_selected(0);
        self.capture = None;
        self.refresh(c);
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
        self.capture = None;
        self.show_capture();
        let _ = self.ui.hide();
        self.restore.take()
    }
    pub fn arrange(&mut self) {
        if !self.opened || !self.pending_window || id(self.ui.window()) == 0 {
            return;
        }
        tool(self.ui.window(), false);
        native::position(id(self.ui.window()), self.monitor, Some(HWND_TOPMOST));
        native::focus(id(self.ui.window()), false);
        self.ui.invoke_focus_search();
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
    pub fn capturing(&self) -> bool {
        self.opened && self.capture.is_some()
    }
    /// Rebuilds the list from the user's file; a completed capture is closed
    /// because the reload that follows a successful write lands here.
    pub fn refresh(&mut self, c: &Config) {
        self.home = c.home.clone();
        let text = crate::files::read_config(&self.home.join("keybindings.toml"))
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
            .unwrap_or_else(|e| {
                tracing::warn!(%e, "keybindings file unreadable");
                String::new()
            });
        self.rows = keybindings::rows(keybindings::DEFAULTS, &text);
        if self.capture.as_ref().is_some_and(|c| c.error.is_none()) {
            self.capture = None;
            self.show_capture();
        }
        let query = self.ui.get_query();
        self.search(query.as_str());
    }
    fn search(&mut self, query: &str) {
        self.shown = keybindings::filter(&self.rows, query);
        let rows: Vec<KeyRow> = self
            .shown
            .iter()
            .map(|index| {
                let row = &self.rows[*index];
                KeyRow {
                    chord: row.label().into(),
                    description: row.description.clone().into(),
                    changed: row.changed(),
                    unbound: row.chord.is_none(),
                }
            })
            .collect();
        super::sync(&self.model, &rows);
        let selected = self.ui.get_selected();
        self.ui
            .set_selected(selected.clamp(0, rows.len().saturating_sub(1) as i32));
    }
    fn begin(&mut self, shown: i32, reset: bool) {
        let Some(row) = usize::try_from(shown)
            .ok()
            .and_then(|i| self.shown.get(i).copied())
        else {
            return;
        };
        let chord = if reset {
            self.rows[row]
                .default
                .as_deref()
                .and_then(|d| keyboard::chord(d).ok())
        } else {
            None
        };
        self.held = 0;
        self.capture = Some(Capture {
            row,
            chord,
            conflict: None,
            reset,
            error: None,
        });
        self.check_conflict();
        self.show_capture();
    }
    fn check_conflict(&mut self) {
        let Some(capture) = &mut self.capture else {
            return;
        };
        capture.conflict = capture
            .chord
            .and_then(|(vk, modifiers)| keyboard::format(vk, modifiers))
            .and_then(|chord| keybindings::conflict(&self.rows, &chord, capture.row));
    }
    fn show_capture(&self) {
        let Some(capture) = &self.capture else {
            self.ui.set_capture_open(false);
            return;
        };
        let row = &self.rows[capture.row];
        let chord = capture
            .chord
            .and_then(|(vk, modifiers)| keyboard::format(vk, modifiers));
        let chips: Vec<slint::SharedString> = match (&chord, self.held) {
            (Some(chord), _) => chord.split('+').map(Into::into).collect(),
            (None, held) => modifier_names(held).into_iter().map(Into::into).collect(),
        };
        let (message, warning) = if let Some(error) = &capture.error {
            (error.clone(), true)
        } else if let Some(other) = capture.conflict {
            (
                format!(
                    "Already bound to {}. Enter replaces it, Esc keeps it.",
                    self.rows[other].description
                ),
                true,
            )
        } else if capture.reset {
            ("Enter restores the default chord.".into(), false)
        } else {
            (String::new(), false)
        };
        self.ui.set_capture_open(true);
        self.ui.set_capture_title(
            if capture.reset {
                format!("Reset {}", row.description)
            } else {
                row.description.clone()
            }
            .into(),
        );
        self.ui.set_capture_chord(
            chord
                .as_deref()
                .map(keybindings::pretty)
                .unwrap_or_default()
                .into(),
        );
        self.ui
            .set_capture_chips(ModelRc::from(Rc::new(VecModel::from(chips))));
        self.ui.set_capture_message(message.into());
        self.ui.set_capture_warning(warning);
        self.ui.set_capture_hint(
            if capture.conflict.is_some() {
                "Enter replace · Esc cancel · Backspace clear"
            } else {
                "Enter apply · Esc cancel · Backspace clear"
            }
            .into(),
        );
    }
    /// A write that was rejected keeps the dialog open with the reason.
    pub fn fail(&mut self, error: String) {
        if let Some(capture) = &mut self.capture {
            capture.error = Some(error);
            self.show_capture();
        }
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        match input {
            Input::Search(query) => {
                self.search(&query);
                Outcome::None
            }
            Input::Change(n) if self.capture.is_none() => {
                self.begin(n, false);
                Outcome::None
            }
            Input::Reset(n) if self.capture.is_none() => {
                self.begin(n, true);
                Outcome::None
            }
            Input::Change(_) | Input::Reset(_) => Outcome::None,
            Input::Dismiss if self.capture.is_some() => {
                self.capture = None;
                self.show_capture();
                Outcome::None
            }
            Input::Dismiss => Outcome::Close,
        }
    }
    /// A key reported by the hook while capturing.
    pub fn capture(&mut self, vk: u32, modifiers: u8, down: bool) -> Outcome {
        let Some(capture) = &mut self.capture else {
            return Outcome::None;
        };
        if keyboard::is_modifier(vk) {
            self.held = modifiers;
            self.show_capture();
            return Outcome::None;
        }
        if !down {
            return Outcome::None;
        }
        capture.error = None;
        match (vk, modifiers) {
            (0x1b, 0) => {
                self.capture = None;
                self.show_capture();
                return Outcome::None;
            }
            (0x08, 0) => {
                capture.chord = None;
                capture.conflict = None;
                self.held = 0;
            }
            (0x0d, 0) => {
                if let Some((vk, modifiers)) = capture.chord
                    && let Some(chord) = keyboard::format(vk, modifiers)
                {
                    let row = &self.rows[capture.row];
                    let mut remove: Vec<String> = row.chord.iter().cloned().collect();
                    if let Some(other) = capture.conflict
                        && let Some(taken) = &self.rows[other].chord
                    {
                        remove.push(taken.clone());
                    }
                    return Outcome::Apply(Apply {
                        remove,
                        chord,
                        command: row.command.clone(),
                    });
                }
            }
            (_, 0) => {
                capture.error = Some("Add a modifier: Ctrl, Alt, Shift or Super.".into());
            }
            (vk, modifiers) => match keyboard::key_name(vk) {
                Some(_) => {
                    capture.chord = Some((vk, modifiers));
                    self.check_conflict();
                }
                None => capture.error = Some("This key cannot be bound.".into()),
            },
        }
        self.show_capture();
        Outcome::None
    }
}
