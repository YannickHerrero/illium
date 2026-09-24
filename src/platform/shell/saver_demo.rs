//! Screensaver demo surface: the effects listed with the selected one
//! playing in a preview, or one effect full screen. Ctrl+N / Ctrl+P step
//! through the effects in both modes; Escape leaves full screen, then closes.
use super::{ScreensaverDemoView, color, id, tool};
use crate::{
    config::Config,
    layout::Rect,
    platform::{Event, EventSender, dpi, native},
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use winarchy_config::screensaver::Effect;
use winarchy_screensaver::player::Player;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, HWND_TOPMOST};

#[derive(Clone, Debug)]
pub enum Input {
    /// Key text and whether Ctrl is held.
    Key(String, bool),
    Choose(i32),
    Dismiss,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Close,
}
/// Logical layout of the list view.
const MARGIN: f32 = 48.0;
const LIST_WIDTH: f32 = 260.0;
const HINT_HEIGHT: f32 = 32.0;

pub struct SaverDemo {
    ui: ScreensaverDemoView,
    epoch: Rc<Cell<u64>>,
    pub opened: bool,
    pending_window: bool,
    restore: Option<isize>,
    monitor: Rect,
    player: Option<Player>,
    selected: usize,
    playing: bool,
    last_frame: Instant,
}
impl SaverDemo {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = ScreensaverDemoView::new().map_err(|e| e.to_string())?;
        let names: Vec<SharedString> = Effect::ALL.iter().map(|e| e.name().into()).collect();
        ui.set_names(ModelRc::new(VecModel::from(names)));
        ui.set_margin(MARGIN);
        ui.set_list_width(LIST_WIDTH);
        let epoch = Rc::new(Cell::new(0));
        let send = |tx: &EventSender, epoch: &Rc<Cell<u64>>| {
            let (t, e) = (tx.clone(), epoch.clone());
            move |input: Input| {
                let _ = t.send(Event::SaverDemo(e.get(), input));
            }
        };
        let f = send(&tx, &epoch);
        ui.on_key(move |text, ctrl| f(Input::Key(text.into(), ctrl)));
        let f = send(&tx, &epoch);
        ui.on_choose(move |n| f(Input::Choose(n)));
        let f = send(&tx, &epoch);
        ui.window().on_close_requested(move || {
            f(Input::Dismiss);
            slint::CloseRequestResponse::KeepWindowShown
        });
        Ok(Self {
            ui,
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
            player: None,
            selected: 0,
            playing: false,
            last_frame: Instant::now(),
        })
    }
    /// Opens the list, or one effect full screen when `effect` is given.
    pub fn open(
        &mut self,
        c: &Config,
        monitor: Rect,
        restore: Option<isize>,
        effect: Option<Effect>,
    ) {
        if let Some(effect) = effect {
            self.selected = Effect::ALL.iter().position(|&e| e == effect).unwrap_or(0);
        }
        if self.opened {
            self.playing = effect.is_some();
            self.restart();
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.restore = restore;
        self.monitor = monitor;
        self.ui.set_bg(color(&c.theme.background));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
        let (w, h) = (
            dpi::logical(monitor, monitor.w),
            dpi::logical(monitor, monitor.h),
        );
        self.ui.set_surface_width(w);
        self.ui.set_surface_height(h);
        // The preview keeps the monitor's aspect, so it shows the same grid
        // as full screen, only with a smaller font.
        let available_w = w - LIST_WIDTH - 3.0 * MARGIN;
        let available_h = h - 2.0 * MARGIN - HINT_HEIGHT;
        let preview_w = available_w.min(available_h * w / h).max(1.0);
        let preview_h = preview_w * h / w;
        self.ui.set_preview_x(2.0 * MARGIN + LIST_WIDTH);
        self.ui.set_preview_y((h - HINT_HEIGHT - preview_h) / 2.0);
        self.ui.set_preview_width(preview_w);
        self.ui.set_preview_height(preview_h);
        self.playing = effect.is_some();
        self.restart();
        super::prepare(self.ui.window(), monitor, false);
        if self.ui.show().is_err() {
            self.player = None;
            return;
        }
        self.opened = true;
        self.pending_window = true;
    }
    fn scale(&self) -> f32 {
        self.monitor.w as f32 / dpi::logical(self.monitor, self.monitor.w)
    }
    /// A fresh player for the selected effect at the current mode's size.
    fn restart(&mut self) {
        let effect = Effect::ALL[self.selected];
        let scale = self.scale();
        let (w, h, font_scale) = if self.playing {
            (self.monitor.w as f32, self.monitor.h as f32, scale)
        } else {
            let w = self.ui.get_preview_width() * scale;
            let h = self.ui.get_preview_height() * scale;
            (w, h, scale * w / self.monitor.w.max(1) as f32)
        };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let mut player = Player::new(
            &[effect],
            w.max(1.0) as usize,
            h.max(1.0) as usize,
            font_scale,
            seed,
        );
        player.shuffle = false;
        self.player = Some(player);
        self.ui.set_selected(self.selected as i32);
        self.ui.set_playing(self.playing);
        self.ui.set_frame(Default::default());
        self.last_frame = Instant::now();
    }
    fn select(&mut self, index: usize) {
        self.selected = index % Effect::ALL.len();
        self.restart();
    }
    fn step(&mut self, forward: bool) {
        let n = Effect::ALL.len();
        self.select(if forward {
            self.selected + 1
        } else {
            self.selected + n - 1
        });
    }
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.opened = false;
        self.pending_window = false;
        self.player = None;
        let _ = self.ui.hide();
        self.ui.set_frame(Default::default());
        self.restore.take()
    }
    pub fn arrange(&mut self) {
        if !self.opened || !self.pending_window || id(self.ui.window()) == 0 {
            return;
        }
        tool(self.ui.window(), false);
        native::position(id(self.ui.window()), self.monitor, Some(HWND_TOPMOST));
        native::focus(id(self.ui.window()), false);
        self.ui.invoke_focus_view();
        self.pending_window = false;
    }
    /// Draws the next frame; closes once another window takes the foreground,
    /// so a topmost animation never hides what the user switched to.
    pub fn poll(&mut self) -> Outcome {
        if !self.opened || self.pending_window {
            return Outcome::None;
        }
        let foreground = unsafe { GetForegroundWindow() }.0 as isize;
        if foreground != id(self.ui.window()) {
            return Outcome::Close;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame);
        if elapsed < Duration::from_millis(16) {
            return Outcome::None;
        }
        self.last_frame = now;
        let Some(player) = &mut self.player else {
            return Outcome::None;
        };
        if player.advance(elapsed.as_secs_f64()) {
            let (w, h) = player.size();
            let buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(
                player.pixels(),
                w as u32,
                h as u32,
            );
            self.ui.set_frame(slint::Image::from_rgb8(buffer));
        }
        Outcome::None
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        use slint::platform::Key as K;
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        match input {
            Input::Dismiss => Outcome::Close,
            Input::Choose(n) => {
                self.select(n.max(0) as usize);
                Outcome::None
            }
            Input::Key(text, ctrl) => {
                let key = |k: K| text.starts_with(char::from(k));
                // Ctrl+letter arrives either as the letter or as its control code.
                let ctrl_letter = |letter: char| {
                    ctrl && (text.eq_ignore_ascii_case(&letter.to_string())
                        || text == char::from(letter as u8 - b'a' + 1).to_string())
                };
                if key(K::Escape) {
                    if !self.playing {
                        return Outcome::Close;
                    }
                    self.playing = false;
                    self.restart();
                } else if ctrl_letter('n') || (!self.playing && key(K::DownArrow)) {
                    self.step(true);
                } else if ctrl_letter('p') || (!self.playing && key(K::UpArrow)) {
                    self.step(false);
                } else if !self.playing && key(K::Return) {
                    self.playing = true;
                    self.restart();
                }
                Outcome::None
            }
        }
    }
}
