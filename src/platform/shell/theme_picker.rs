//! Shared theme/wallpaper carousel boundary. No preference application here.
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

#[derive(Clone, PartialEq)]
struct ViewKey {
    home: PathBuf,
    wallpaper_theme: Option<String>,
    active: String,
    width: f32,
    height: f32,
    physical_width: i32,
    colors: Colors,
}
struct View {
    key: ViewKey,
    entries: Vec<Entry>,
    model: Model,
    cards: Vec<Card>,
    frames: Vec<Arc<crate::theme_picker::render::Frame>>,
}
impl View {
    fn bytes(&self) -> usize {
        self.frames.iter().map(|f| f.bytes()).sum()
    }
}

struct Warmup {
    key: ViewKey,
    entries: Vec<Entry>,
    model: Model,
    cards: Vec<Card>,
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
    // At most two initial views, with a shared 64 MiB bound. These retain CPU
    // frames, not an unbounded second set of Slint image buffers.
    views: VecDeque<View>,
    warming: Option<Warmup>,
    warm_attempted: VecDeque<ViewKey>,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    home: PathBuf,
    active: String,
    pub wallpaper_theme: Option<String>,
    invalidated: bool,
    monitor: Rect,
    colors: Colors,
    entries: Vec<Entry>,
    model: Model,
    shown: Model,
    shown_key: Option<ViewKey>,
    hit_cards: Rc<RefCell<Vec<Card>>>,
    serial: u64,
    scanning: bool,
    pending_cards: Option<Vec<Card>>,
    confirm_target: Option<String>,
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
            views: VecDeque::new(),
            warming: None,
            warm_attempted: VecDeque::new(),
            epoch,
            opened: false,
            pending_window: false,
            restore: None,
            home: PathBuf::new(),
            active: String::new(),
            wallpaper_theme: None,
            invalidated: false,
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
            shown_key: None,
            hit_cards,
            serial: 0,
            scanning: false,
            pending_cards: None,
            confirm_target: None,
            error: None,
        })
    }
    pub fn open(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
    ) -> Result<(), String> {
        self.open_images(c, monitor, restore, None, c.global.theme.clone())
    }
    fn open_images(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
        wallpaper_theme: Option<String>,
        active: String,
    ) -> Result<(), String> {
        if self.opened {
            return Ok(());
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        if let Some(warm) = self.warming.take() {
            self.warm_attempted.retain(|key| *key != warm.key);
        }
        self.restore = restore;
        self.home = c.home.clone();
        self.active = active;
        self.wallpaper_theme = wallpaper_theme;
        self.invalidated = false;
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
        self.shown_key = None;
        self.pending_cards = None;
        self.confirm_target = None;
        self.error = None;
        let key = self.view_key();
        if let Some(index) = self.views.iter().position(|v| v.key == key) {
            let view = self.views.remove(index).unwrap();
            self.entries = view.entries.clone();
            self.model = view.model.clone();
            self.show_frames(
                &view.cards,
                view.frames.iter().cloned().map(Some).collect(),
                true,
            );
            self.views.push_back(view);
        }
        self.ui.show().map_err(|e| e.to_string())?;
        self.opened = true;
        self.pending_window = true;
        // A cached view is immediately visible, but confirmation waits for
        // fresh filesystem validation (including edits while closed).
        self.scan();
        Ok(())
    }
    pub fn open_wallpapers(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
        selected: Option<&str>,
    ) -> Result<(), String> {
        self.open_images(
            c,
            monitor,
            restore,
            Some(c.global.theme.clone()),
            selected.unwrap_or_default().to_owned(),
        )
    }
    /// A filename from an old theme must never be applied to a new theme.
    pub fn selection_command(
        &self,
        name: String,
        current_theme: &str,
    ) -> Option<crate::command::Command> {
        use crate::command::Command;
        match self.wallpaper_theme.as_deref() {
            None => Some(Command::Theme(name)),
            Some(theme) if theme == current_theme && !self.invalidated => {
                Some(Command::Wallpaper(Some(name)))
            }
            Some(_) => None,
        }
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
        self.confirm_target = None;
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
        if let Some(theme) = &self.wallpaper_theme {
            self.invalidated |= self.opened && theme != &c.global.theme;
        } else {
            self.active = c.global.theme.clone();
        }
        self.colors = Colors::from_theme(&c.theme);
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_fg(color(&c.theme.text));
        if self.opened {
            self.rescan();
        }
    }
    pub fn rescan(&mut self) {
        self.views.clear();
        self.warm_attempted.clear();
        if self.warming.take().is_some() {
            self.loader.cancel();
        }
        self.scan();
    }
    fn scan(&mut self) {
        if !self.opened {
            return;
        }
        self.scanning = true;
        self.confirm_target = None;
        self.pending_cards = None;
        let job = match &self.wallpaper_theme {
            Some(theme) => Job::Wallpapers(self.home.clone(), theme.clone()),
            None => Job::Scan(self.home.clone()),
        };
        self.serial = self.loader.request(job);
    }
    fn view_key(&self) -> ViewKey {
        let (width, height) = self.dimensions();
        ViewKey {
            home: self.home.clone(),
            wallpaper_theme: self.wallpaper_theme.clone(),
            active: self.active.clone(),
            width,
            height,
            physical_width: self.monitor.w,
            colors: self.colors,
        }
    }
    fn remember_view(
        &mut self,
        cards: &[Card],
        frames: &[Arc<crate::theme_picker::render::Frame>],
    ) {
        let initial = Model::new(self.model.ids.clone(), &self.active);
        if self.model != initial {
            return;
        }
        let key = self.view_key();
        let view = View {
            key: key.clone(),
            entries: self.entries.clone(),
            model: self.model.clone(),
            cards: cards.to_vec(),
            frames: frames.to_vec(),
        };
        self.store_view(view);
    }
    fn store_view(&mut self, view: View) {
        const LIMIT: usize = 64 * 1024 * 1024;
        if view.bytes() > LIMIT {
            return;
        }
        self.views.retain(|v| v.key != view.key);
        while self.views.len() >= 2
            || self.views.iter().map(View::bytes).sum::<usize>() + view.bytes() > LIMIT
        {
            self.views.pop_front();
        }
        self.views.push_back(view);
    }
    /// Called only during shell idle time. Never creates/shows/focuses a window
    /// or changes preferences. Uses the same bounded worker and view caches.
    pub fn preload(&mut self, c: &Config, monitor: Rect, selected: Option<&str>) {
        if self.opened {
            return;
        }
        let theme_key = ViewKey {
            home: c.home.clone(),
            wallpaper_theme: None,
            active: c.global.theme.clone(),
            width: dpi::logical(monitor, monitor.w),
            height: dpi::logical(monitor, monitor.h),
            physical_width: monitor.w,
            colors: Colors::from_theme(&c.theme),
        };
        let wallpaper_key = ViewKey {
            wallpaper_theme: Some(c.global.theme.clone()),
            active: selected.unwrap_or_default().into(),
            ..theme_key.clone()
        };
        let keys = [theme_key, wallpaper_key];
        if let Some(warm) = &self.warming {
            if keys.contains(&warm.key) {
                return;
            }
            self.loader.cancel();
            self.warming = None;
        }
        let Some(key) = keys.into_iter().find(|key| {
            !self.views.iter().any(|v| &v.key == key) && !self.warm_attempted.contains(key)
        }) else {
            return;
        };
        self.warm_attempted.push_back(key.clone());
        while self.warm_attempted.len() > 2 {
            self.warm_attempted.pop_front();
        }
        let job = match &key.wallpaper_theme {
            Some(theme) => Job::Wallpapers(key.home.clone(), theme.clone()),
            None => Job::Scan(key.home.clone()),
        };
        self.warming = Some(Warmup {
            key,
            entries: vec![],
            model: Model::default(),
            cards: vec![],
        });
        self.serial = self.loader.request(job);
    }
    fn poll_warmup(&mut self, result: Result<Output, String>) {
        let Some(mut warm) = self.warming.take() else {
            return;
        };
        match result {
            Ok(Output::Catalog(entries)) => {
                warm.model = Model::new(
                    entries.iter().map(|e| e.id.clone()).collect(),
                    &warm.key.active,
                );
                warm.entries = entries;
            }
            Ok(Output::Unreadable(entry, _)) => {
                warm.entries.retain(|e| *e != entry);
                warm.model
                    .replace(warm.entries.iter().map(|e| e.id.clone()).collect());
            }
            Ok(Output::Progress(_)) => {
                self.warming = Some(warm);
                return;
            }
            Ok(Output::Frames(frames)) => {
                self.store_view(View {
                    key: warm.key,
                    entries: warm.entries,
                    model: warm.model,
                    cards: warm.cards,
                    frames,
                });
                return;
            }
            Err(error) => {
                tracing::debug!(%error, "picker preload skipped");
                return;
            }
        }
        warm.cards = warm.model.cards(warm.key.width, warm.key.height);
        let dpi = (96.0 * warm.key.physical_width as f32 / warm.key.width).round() as u32;
        let keys = warm
            .cards
            .iter()
            .map(|card| Key {
                entry: warm.entries[card.index].clone(),
                dpi,
                selected: card.selected,
                colors: warm.key.colors,
            })
            .collect();
        self.serial = self.loader.request(Job::Render(keys));
        self.warming = Some(warm);
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
        if self.invalidated {
            return Outcome::Cancel;
        }
        if !matches!(input, Input::Action(Action::Confirm)) {
            self.confirm_target = None;
        }
        let result = match input {
            Input::Cancel => Outcome::Cancel,
            Input::Click(x, y) => {
                let (w, h) = self.dimensions();
                // Mouse actions refer to what is actually drawn, not a queued
                // keyboard selection whose pixels have not arrived yet.
                let hit = self
                    .hit_cards
                    .borrow()
                    .iter()
                    .rev()
                    .find(|c| c.contains(x, y))
                    .map(|c| c.index);
                let mut clicked = self.shown.clone();
                let result = if let Some(index) = hit {
                    clicked.action(Action::Click(index))
                } else if self.shown.cards(w, h).iter().any(|c| c.contains(x, y)) {
                    // A progressive card that has no pixels yet cannot be clicked.
                    Outcome::None
                } else {
                    clicked.click(x, y, w, h)
                };
                // Keep `shown` authoritative until pixels actually arrive;
                // catalog revalidation must notice a click changed selection.
                self.model = clicked;
                self.model
                    .replace(self.entries.iter().map(|e| e.id.clone()).collect());
                result
            }
            Input::Action(Action::Confirm)
                if (self.scanning || self.pending_cards.is_some())
                    && self.model.selected().is_some() =>
            {
                self.confirm_target = self.model.selected_id().map(str::to_owned);
                return Outcome::None;
            }
            Input::Action(action) => self.model.action(action),
        };
        if let Outcome::Apply(ref target) = result
            && (self.scanning || self.pending_cards.is_some())
        {
            self.confirm_target = Some(target.clone());
            // A click can target the previous displayed selection while a
            // different keyboard selection is still rendering.
            if !self.scanning {
                self.render();
            }
            return Outcome::None;
        }
        if result == Outcome::None && !self.scanning {
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
    fn show_frames(
        &mut self,
        cards: &[Card],
        frames: Vec<Option<Arc<crate::theme_picker::render::Frame>>>,
        complete: bool,
    ) {
        let mut visible = Vec::new();
        let rows: Vec<_> = cards
            .iter()
            .zip(frames)
            .filter_map(|(card, frame)| {
                let frame = frame?;
                visible.push(card.clone());
                Some(PreviewCard {
                    image: self.image(&frame),
                    x: card.x - PAD,
                    y: card.y - PAD,
                    width: card.width + 2.0 * PAD,
                    height: card.height + 2.0 * PAD,
                })
            })
            .collect();
        super::sync(&self.rows, &rows);
        let label = if self.wallpaper_theme.is_some() {
            self.model
                .selected_id()
                .map(str::to_owned)
                .unwrap_or_else(|| self.model.current_label())
        } else {
            self.model.current_label()
        };
        self.ui.set_selected_label(label.into());
        self.ui.set_filter_text(self.model.filter.clone().into());
        self.ui.set_content_ready(!self.model.ids.is_empty());
        *self.hit_cards.borrow_mut() = visible;
        self.shown = self.model.clone();
        self.shown_key = complete.then(|| self.view_key());
    }
    pub fn poll(&mut self) -> Outcome {
        if self.opened && self.invalidated {
            return Outcome::Cancel;
        }
        let Some(completion) = self.loader.take_result() else {
            return Outcome::None;
        };
        if completion.serial != self.serial {
            return Outcome::None;
        }
        if !self.opened {
            self.poll_warmup(completion.result);
            return Outcome::None;
        }
        match completion.result {
            Ok(Output::Catalog(entries)) => {
                let unchanged = self.entries == entries
                    && self.model == self.shown
                    && self.ui.get_content_ready()
                    && self.shown_key.as_ref() == Some(&self.view_key());
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
                if unchanged {
                    if let Some(target) = self.confirm_target.take()
                        && self.model.selected_id() == Some(target.as_str())
                    {
                        return Outcome::Apply(target);
                    }
                } else {
                    self.render();
                }
            }
            Ok(Output::Progress(frames)) => {
                if let Some(cards) = self.pending_cards.clone() {
                    self.show_frames(&cards, frames, false);
                }
            }
            Ok(Output::Frames(frames)) => {
                let Some(cards) = self.pending_cards.take() else {
                    return Outcome::None;
                };
                self.remember_view(&cards, &frames);
                self.show_frames(&cards, frames.into_iter().map(Some).collect(), true);
                if let Some(target) = self.confirm_target.take()
                    && self.model.selected_id() == Some(target.as_str())
                {
                    return Outcome::Apply(target);
                }
            }
            Ok(Output::Unreadable(entry, error)) => {
                tracing::warn!(item=%entry.id,%error,"image preview unavailable");
                self.error = Some(error);
                // Never confirm the replacement of an unreadable selected image.
                self.confirm_target = None;
                self.entries.retain(|e| *e != entry);
                self.model
                    .replace(self.entries.iter().map(|e| e.id.clone()).collect());
                self.render();
            }
            Err(error) => {
                tracing::warn!(%error,"image picker failed");
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
