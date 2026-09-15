//! The native/UI boundary of the theme picker. No theme application here.
use super::{PreviewCard, ThemePicker, color, id, tool};
use crate::{
    config::Config,
    layout::Rect,
    platform::{Event, EventSender, dpi, native},
    theme_picker::{
        Action, Card, Model, Outcome,
        loader::{Job, Loader, Output},
        render::{Colors, Key, PAD},
    },
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Weak},
};
use winarchy_theme::preview::Entry;
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

#[derive(Clone, Debug)]
pub enum Input {
    Action(Action),
    Click(f32, f32),
    Cancel,
}

fn key(text: &str, control: bool, shift: bool, alt: bool, meta: bool) -> Option<Action> {
    use slint::platform::Key as K;
    let c = text.chars().next()?;
    let special = |k: K| char::from(k) == c;
    Some(if special(K::Escape) {
        Action::Escape
    } else if special(K::Return) {
        Action::Confirm
    } else if special(K::Backspace) && !alt && !meta {
        if control {
            Action::DeleteWord
        } else {
            Action::Backspace
        }
    } else if control && !shift && !alt && !meta && c.eq_ignore_ascii_case(&'u') {
        Action::Clear
    } else if special(K::LeftArrow) || (special(K::Tab) && shift) || special(K::Backtab) {
        Action::Previous
    } else if special(K::RightArrow) || special(K::Tab) {
        Action::Next
    } else if !control
        && !alt
        && !meta
        && !c.is_control()
        && !(('\u{e000}'..='\u{f8ff}').contains(&c))
    {
        Action::Text(text.into())
    } else {
        return None;
    })
}

pub struct Picker {
    ui: ThemePicker,
    rows: Rc<VecModel<PreviewCard>>,
    // Slint buffers are separate from the worker's CPU buffers. Retain uploads
    // across navigation, with their own 64 MiB bound and no strong worker refs.
    image_cache: VecDeque<(
        Weak<crate::theme_picker::render::Frame>,
        slint::Image,
        usize,
    )>,
    loader: Loader,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    home: PathBuf,
    active: String,
    monitor: Rect,
    colors: Colors,
    entries: Vec<Entry>,
    model: Model,
    shown: Model,
    hit_cards: Rc<RefCell<Vec<Card>>>,
    serial: u64,
    scanning: bool,
    pending_cards: Option<Vec<Card>>,
    confirm_pending: bool,
    pub error: Option<String>,
}
impl Picker {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = ThemePicker::new().map_err(|e| e.to_string())?;
        let rows = Rc::new(VecModel::default());
        ui.set_cards(ModelRc::from(rows.clone()));
        let epoch = Rc::new(Cell::new(0));
        let t = tx.clone();
        let e = epoch.clone();
        ui.on_key(move |text, ctrl, shift, alt, meta| {
            if let Some(action) = key(&text, ctrl, shift, alt, meta) {
                let _ = t.send(Event::Picker(e.get(), Input::Action(action)));
            }
        });
        let t = tx.clone();
        let e = epoch.clone();
        ui.on_click(move |x, y| {
            let _ = t.send(Event::Picker(e.get(), Input::Click(x, y)));
        });
        let t = tx;
        let e = epoch.clone();
        ui.window().on_close_requested(move || {
            let _ = t.send(Event::Picker(e.get(), Input::Cancel));
            slint::CloseRequestResponse::KeepWindowShown
        });
        let hit_cards = Rc::new(RefCell::new(Vec::<Card>::new()));
        let cards = hit_cards.clone();
        let weak = ui.as_weak();
        ui.on_pointer_moved(move |x, y| {
            if let Some(ui) = weak.upgrade() {
                ui.set_pointing(cards.borrow().iter().any(|c| c.contains(x, y)));
            }
        });
        Ok(Self {
            ui,
            rows,
            image_cache: VecDeque::new(),
            loader: Loader::default(),
            epoch,
            opened: false,
            pending_window: false,
            restore: None,
            home: PathBuf::new(),
            active: String::new(),
            monitor: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            colors: Colors::from_theme(&winarchy_theme::Theme::default_theme()),
            entries: vec![],
            model: Model::default(),
            shown: Model::default(),
            hit_cards,
            serial: 0,
            scanning: false,
            pending_cards: None,
            confirm_pending: false,
            error: None,
        })
    }
    pub fn open(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
    ) -> Result<(), String> {
        if self.opened {
            return Ok(());
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.restore = restore;
        self.home = c.home.clone();
        self.active = c.global.theme.clone();
        self.monitor = monitor;
        self.colors = Colors::from_theme(&c.theme);
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_surface_width(dpi::logical(monitor, monitor.w));
        self.ui.set_surface_height(dpi::logical(monitor, monitor.h));
        self.ui.set_content_ready(false);
        self.ui.set_pointing(false);
        self.rows.set_vec(vec![]);
        self.hit_cards.borrow_mut().clear();
        self.model = Model::default();
        self.shown = Model::default();
        self.pending_cards = None;
        self.confirm_pending = false;
        self.error = None;
        self.ui.show().map_err(|e| e.to_string())?;
        self.opened = true;
        self.pending_window = true;
        self.rescan();
        Ok(())
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.opened = false;
        self.pending_window = false;
        self.loader.cancel();
        self.pending_cards = None;
        self.scanning = false;
        self.confirm_pending = false;
        let _ = self.ui.hide();
        self.rows.set_vec(vec![]);
        self.hit_cards.borrow_mut().clear();
        self.restore.take()
    }
    pub fn arrange(&mut self) {
        if !self.opened || !self.pending_window || id(self.ui.window()) == 0 {
            return;
        }
        tool(self.ui.window(), false);
        native::position(id(self.ui.window()), self.monitor, Some(HWND_TOPMOST));
        native::focus(id(self.ui.window()), false);
        self.ui.invoke_focus_picker();
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
        if !self.scanning {
            self.render();
        }
    }
    pub fn apply_theme(&mut self, c: &Config) {
        self.home = c.home.clone();
        self.active = c.global.theme.clone();
        self.colors = Colors::from_theme(&c.theme);
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_fg(color(&c.theme.text));
        if self.opened {
            self.rescan();
        }
    }
    pub fn rescan(&mut self) {
        if !self.opened {
            return;
        }
        self.scanning = true;
        self.confirm_pending = false;
        self.pending_cards = None;
        self.serial = self.loader.request(Job::Scan(self.home.clone()));
    }
    fn dimensions(&self) -> (f32, f32) {
        (self.ui.get_surface_width(), self.ui.get_surface_height())
    }
    fn render(&mut self) {
        let (w, h) = self.dimensions();
        let cards = self.model.cards(w, h);
        let dpi = (96.0 * self.monitor.w as f32 / w).round() as u32;
        let keys = cards
            .iter()
            .map(|card| Key {
                entry: self.entries[card.index].clone(),
                dpi,
                selected: card.selected,
                colors: self.colors,
            })
            .collect();
        self.pending_cards = Some(cards);
        self.serial = self.loader.request(Job::Render(keys));
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        let result = match input {
            Input::Cancel => Outcome::Cancel,
            Input::Click(x, y) => {
                let (w, h) = self.dimensions();
                // Mouse actions refer to what is actually drawn, not a queued
                // keyboard selection whose pixels have not arrived yet.
                let result = self.shown.click(x, y, w, h);
                self.model = self.shown.clone();
                self.model
                    .replace(self.entries.iter().map(|e| e.id.clone()).collect());
                result
            }
            Input::Action(Action::Confirm)
                if self.pending_cards.is_some() && self.model.selected().is_some() =>
            {
                self.confirm_pending = true;
                return Outcome::None;
            }
            Input::Action(action) => self.model.action(action),
        };
        if result == Outcome::None && !self.scanning {
            self.confirm_pending = false;
            self.render();
        }
        result
    }
    fn image(&mut self, frame: &Arc<crate::theme_picker::render::Frame>) -> slint::Image {
        let weak = Arc::downgrade(frame);
        if let Some(index) = self
            .image_cache
            .iter()
            .position(|(f, _, _)| f.ptr_eq(&weak))
        {
            let row = self.image_cache.remove(index).unwrap();
            let image = row.1.clone();
            self.image_cache.push_back(row);
            return image;
        }
        self.image_cache
            .retain(|(frame, _, _)| frame.strong_count() > 0);
        let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &frame.pixels,
            frame.width,
            frame.height,
        );
        let image = slint::Image::from_rgba8_premultiplied(buffer);
        const LIMIT: usize = 64 * 1024 * 1024;
        if frame.bytes() <= LIMIT {
            let mut bytes: usize = self.image_cache.iter().map(|(_, _, b)| b).sum();
            while bytes + frame.bytes() > LIMIT {
                let Some((_, _, size)) = self.image_cache.pop_front() else {
                    break;
                };
                bytes -= size;
            }
            self.image_cache
                .push_back((weak, image.clone(), frame.bytes()));
        }
        image
    }
    pub fn poll(&mut self) -> Outcome {
        let Some(completion) = self.loader.take_result() else {
            return Outcome::None;
        };
        if !self.opened || completion.serial != self.serial {
            return Outcome::None;
        }
        match completion.result {
            Ok(Output::Catalog(entries)) => {
                self.entries = entries;
                let ids = self.entries.iter().map(|e| e.id.clone()).collect();
                if self.scanning && self.model.ids.is_empty() {
                    let filter = self.model.filter.clone();
                    self.model = Model::new(ids, &self.active);
                    self.model.action(Action::Text(filter));
                } else {
                    self.model.replace(ids);
                }
                self.scanning = false;
                self.render();
            }
            Ok(Output::Frames(frames)) => {
                let Some(cards) = self.pending_cards.take() else {
                    return Outcome::None;
                };
                let rows: Vec<_> = cards
                    .iter()
                    .zip(frames)
                    .map(|(card, frame)| PreviewCard {
                        image: self.image(&frame),
                        x: card.x - PAD,
                        y: card.y - PAD,
                        width: card.width + 2.0 * PAD,
                        height: card.height + 2.0 * PAD,
                    })
                    .collect();
                super::sync(&self.rows, &rows);
                self.ui
                    .set_selected_label(self.model.current_label().into());
                self.ui.set_filter_text(self.model.filter.clone().into());
                self.ui.set_content_ready(!self.model.ids.is_empty());
                *self.hit_cards.borrow_mut() = cards;
                self.shown = self.model.clone();
                if self.confirm_pending {
                    self.confirm_pending = false;
                    return self.model.action(Action::Confirm);
                }
            }
            Ok(Output::Unreadable(entry, error)) => {
                tracing::warn!(theme=%entry.id,%error,"theme preview unavailable");
                self.error = Some(error);
                // Never confirm the replacement of an unreadable selected theme.
                self.confirm_pending = false;
                self.entries.retain(|e| *e != entry);
                self.model
                    .replace(self.entries.iter().map(|e| e.id.clone()).collect());
                self.render();
            }
            Err(error) => {
                tracing::warn!(%error,"theme picker failed");
                self.error = Some(error);
                return Outcome::Cancel;
            }
        }
        Outcome::None
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.shown.selected_id()
    }
    pub fn filter(&self) -> &str {
        &self.shown.filter
    }
    pub fn loading(&self) -> bool {
        self.scanning || self.pending_cards.is_some()
    }
}

#[cfg(test)]
#[path = "theme_picker_tests.rs"]
mod view_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_match_upstream_without_fuzzy_or_vim_shortcuts() {
        use slint::platform::Key as K;
        let k = |k: K| char::from(k).to_string();
        assert_eq!(
            key(&k(K::Tab), false, true, false, false),
            Some(Action::Previous)
        );
        assert_eq!(
            key(&k(K::Backspace), true, false, false, false),
            Some(Action::DeleteWord)
        );
        assert_eq!(key("u", true, false, false, false), Some(Action::Clear));
        assert_eq!(key("U", true, true, false, false), None);
        assert_eq!(
            key("h", false, false, false, false),
            Some(Action::Text("h".into()))
        );
        assert_eq!(
            key("é", false, false, false, false),
            Some(Action::Text("é".into()))
        );
        assert_eq!(key(&k(K::UpArrow), false, false, false, false), None);
    }
}
