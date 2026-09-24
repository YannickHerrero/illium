//! Lock surfaces: one topmost full-monitor window per display over the blurred
//! wallpaper. Only the surface of the active monitor takes the password; the
//! others cannot be activated.
#[cfg(test)]
#[path = "lockscreen_tests.rs"]
mod tests;
use super::{LockView, SaverRow, color, id, prepare, tool};
use crate::{
    config::Config,
    layout::Rect,
    lockscreen::{Attempts, Verdict},
    platform::{Event, EventSender, dpi, native, status},
};
use crate::{platform::input, screensaver::Saver};
use slint::{ComponentHandle, winit_030::WinitWindowAccessor};
use std::{cell::Cell, rc::Rc, sync::atomic::Ordering, time::Instant};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

fn show_saver(view: &LockView, saving: bool) {
    view.set_saving(saving);
    // Also update a stationary cursor, without modifying Windows' global
    // ShowCursor counter. The Slint overlay maintains this on pointer events.
    view.window()
        .with_winit_window(|window| window.set_cursor_visible(!saving));
}

#[derive(Clone, Debug)]
pub enum Input {
    Submit(String),
    /// Focused Slint surface fallback when Windows drops the keyboard hook.
    Wake,
    /// The Windows lock took over: the surfaces can go.
    Release,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Unlock,
    /// Hand over to the Windows lock.
    Fallback,
}
pub struct Lock {
    views: Vec<LockView>,
    monitors: Vec<Rect>,
    primary: usize,
    epoch: Rc<Cell<u64>>,
    tx: EventSender,
    pub opened: bool,
    pending_window: bool,
    /// Waiting for the Windows lock to cover the desktop before closing.
    pub released: bool,
    hash: String,
    attempts: Attempts,
    restore: Option<isize>,
    saver: Option<Saver>,
    next_frame: Instant,
    last_poll: Instant,
}
impl Lock {
    pub fn new(tx: EventSender) -> Self {
        Self {
            views: vec![],
            monitors: vec![],
            primary: 0,
            epoch: Rc::new(Cell::new(0)),
            tx,
            opened: false,
            pending_window: false,
            released: false,
            hash: String::new(),
            attempts: Attempts::default(),
            restore: None,
            saver: None,
            next_frame: Instant::now(),
            last_poll: Instant::now(),
        }
    }
    fn view(&self) -> Result<LockView, String> {
        let ui = LockView::new().map_err(|e| e.to_string())?;
        let send = |epoch: &Rc<Cell<u64>>| {
            let (tx, epoch) = (self.tx.clone(), epoch.clone());
            move |input: Input| {
                let _ = tx.send(Event::Lock(epoch.get(), input));
            }
        };
        let f = send(&self.epoch);
        ui.on_submit(move |text| f(Input::Submit(text.into())));
        let f = send(&self.epoch);
        ui.on_wake(move || f(Input::Wake));
        ui.window()
            .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
        Ok(ui)
    }
    pub fn open(
        &mut self,
        c: &Config,
        monitors: &[Rect],
        primary: usize,
        backdrops: Vec<slint::Image>,
        hash: String,
        restore: Option<isize>,
    ) -> Result<(), String> {
        if self.opened {
            return Ok(());
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.hash = hash;
        self.attempts = Attempts::default();
        self.restore = restore;
        self.released = false;
        self.opened = true;
        let now = Instant::now();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        self.saver = Some(Saver::new(
            c.global.screensaver.clone(),
            now,
            input::LOCK_ACTIVITY.load(Ordering::SeqCst),
            seed,
        ));
        self.next_frame = now;
        self.last_poll = now;
        self.place(c, monitors, primary, backdrops)
    }
    /// Shows one surface per monitor; also used when the displays change.
    pub fn place(
        &mut self,
        c: &Config,
        monitors: &[Rect],
        primary: usize,
        backdrops: Vec<slint::Image>,
    ) -> Result<(), String> {
        while self.views.len() < monitors.len() {
            let view = self.view()?;
            self.views.push(view);
        }
        for view in &self.views[monitors.len()..] {
            let _ = view.hide();
        }
        self.monitors = monitors.to_vec();
        self.primary = primary.min(monitors.len().saturating_sub(1));
        for (index, (view, monitor)) in self.views.iter().zip(monitors).enumerate() {
            view.set_bg(color(&c.theme.background));
            view.set_fg(color(&c.theme.text));
            view.set_muted(color(&c.theme.subtext));
            view.set_accent(color(&c.theme.accent));
            view.set_surface_width(dpi::logical(*monitor, monitor.w));
            view.set_surface_height(dpi::logical(*monitor, monitor.h));
            let backdrop = backdrops.get(index).cloned().unwrap_or_default();
            view.set_has_backdrop(backdrop.size().width > 0);
            view.set_backdrop(backdrop);
            view.set_primary(index == self.primary);
            show_saver(view, input::SAVING.load(Ordering::SeqCst));
            view.set_ready(false);
            prepare(view.window(), *monitor, index != self.primary);
            view.show().map_err(|e| e.to_string())?;
        }
        self.tick();
        self.pending_window = true;
        Ok(())
    }
    pub fn arrange(&mut self) {
        let shown = &self.views[..self.monitors.len().min(self.views.len())];
        if !self.opened || !self.pending_window || shown.iter().any(|v| id(v.window()) == 0) {
            return;
        }
        for (index, (view, monitor)) in shown.iter().zip(&self.monitors).enumerate() {
            tool(view.window(), index != self.primary);
            native::position(id(view.window()), *monitor, Some(HWND_TOPMOST));
            view.set_ready(true);
            // A window shown again starts from an empty surface and Slint only
            // repaints what changed: `ready` went false then true without a
            // frame in between, which leaves all but the focused field blank.
            view.set_repaint_nonce(view.get_repaint_nonce().wrapping_add(1));
        }
        if let Some(view) = shown.get(self.primary) {
            native::focus(id(view.window()), false);
            if view.get_saving() {
                view.invoke_focus_saver();
            } else {
                view.invoke_focus_field();
            }
        }
        self.pending_window = false;
    }
    /// The surface that takes the password, 0 while closed.
    pub fn primary_hwnd(&self) -> isize {
        match self.views.get(self.primary) {
            Some(view) if self.opened => id(view.window()),
            _ => 0,
        }
    }
    pub fn owns(&self, window: isize) -> bool {
        self.opened && self.views.iter().any(|v| id(v.window()) == window)
    }
    pub fn tick(&self) {
        if !self.opened {
            return;
        }
        let now = status::now();
        let time = crate::clock::format("%H:%M", now);
        let date = crate::clock::format("%A %d %B", now);
        for view in &self.views {
            view.set_time(time.clone().into());
            view.set_date(date.clone().into());
        }
    }
    /// Called by the existing shell poll; does no frame work while closed/idle.
    pub fn poll(&mut self) {
        if !self.opened || self.released || !input::MOUSE_AVAILABLE.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        // Suspend/resume (or a long event-loop stall) starts a fresh idle
        // interval instead of immediately advancing an old animation.
        let resumed = now.duration_since(self.last_poll) > std::time::Duration::from_secs(2);
        self.last_poll = now;
        // A secure desktop (Windows lock/UAC) or focus handover must not
        // animate invisibly or bring our password surface back to the front.
        let foreground =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0 as isize;
        let focused = foreground != 0 && foreground == self.primary_hwnd();
        let Some(saver) = &mut self.saver else {
            return;
        };
        if !focused || resumed {
            saver.reset(now, input::LOCK_ACTIVITY.load(Ordering::SeqCst));
            if input::SAVING.swap(false, Ordering::SeqCst) {
                for view in &self.views {
                    show_saver(view, false);
                    view.set_saver_rows(Default::default());
                    view.invoke_clear_field();
                    // Restore logical focus only, never activate a window on
                    // top of the secure desktop or another foreground owner.
                    if view.get_primary() {
                        view.invoke_focus_field();
                    }
                }
            }
            return;
        }
        let changed = saver.poll(now, input::LOCK_ACTIVITY.load(Ordering::SeqCst));
        let saving = saver.effect.is_some();
        if changed {
            // Arm input swallowing before displaying the animation. On wake,
            // leave it armed until the field and focus have been restored.
            if saving {
                input::SAVING.store(true, Ordering::SeqCst);
            }
            for view in &self.views {
                view.invoke_clear_field();
                show_saver(view, saving);
                if !saving {
                    view.set_saver_rows(Default::default());
                }
            }
            if let Some(view) = self.views.get(self.primary) {
                if saving {
                    view.invoke_focus_saver();
                } else {
                    view.invoke_focus_field();
                }
            }
            if !saving {
                input::SAVING.store(false, Ordering::SeqCst);
            }
            self.next_frame = now;
        }
        if saving && now >= self.next_frame {
            self.next_frame = now + std::time::Duration::from_millis(33);
            for (index, view) in self.views.iter().take(self.monitors.len()).enumerate() {
                let rows: Vec<_> = saver
                    .frame(now, index)
                    .into_iter()
                    .map(|row| SaverRow {
                        text: row.text.into(),
                        intensity: row.intensity,
                        blend: row.blend,
                    })
                    .collect();
                view.set_saver_rows(Rc::new(slint::VecModel::from(rows)).into());
            }
        }
    }
    fn wake(&mut self) {
        let Some(saver) = &mut self.saver else {
            return;
        };
        if saver.effect.is_none() {
            return;
        }
        saver.reset(Instant::now(), input::LOCK_ACTIVITY.load(Ordering::SeqCst));
        for view in &self.views {
            view.invoke_clear_field();
            show_saver(view, false);
            view.set_saver_rows(Default::default());
        }
        if let Some(view) = self.views.get(self.primary) {
            view.invoke_focus_field();
        }
        input::SAVING.store(false, Ordering::SeqCst);
    }
    fn stop_saver(&mut self) {
        self.saver = None;
        for view in &self.views {
            show_saver(view, false);
            view.set_saver_rows(Default::default());
            view.invoke_reset_input();
        }
        input::SAVING.store(false, Ordering::SeqCst);
    }
    /// The Windows lock was requested; the surfaces close once it covers the
    /// desktop, so nothing shows in between.
    pub fn release_later(&mut self) {
        self.stop_saver();
        self.released = true;
        let (tx, epoch) = (self.tx.clone(), self.epoch.get());
        slint::Timer::single_shot(std::time::Duration::from_millis(1500), move || {
            let _ = tx.send(Event::Lock(epoch, Input::Release));
        });
    }
    pub fn input(&mut self, epoch: u64, input: Input) -> Outcome {
        if !self.opened || epoch != self.epoch.get() {
            return Outcome::None;
        }
        match input {
            Input::Release => Outcome::Unlock,
            _ if self.released => Outcome::None,
            Input::Wake => {
                self.wake();
                Outcome::None
            }
            _ if input::SAVING.load(Ordering::SeqCst) => Outcome::None,
            Input::Submit(password) => match self.attempts.check(&self.hash, &password) {
                Verdict::Unlock => Outcome::Unlock,
                Verdict::Wrong { remaining } => {
                    let message = match remaining {
                        1 => "Wrong password, 1 attempt left".to_owned(),
                        n => format!("Wrong password, {n} attempts left"),
                    };
                    for view in &self.views {
                        view.set_message(message.clone().into());
                    }
                    Outcome::None
                }
                Verdict::Exhausted => Outcome::Fallback,
            },
        }
    }
    /// Hides every surface and returns the window to focus again.
    pub fn close(&mut self) -> Option<isize> {
        if !self.opened {
            return None;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.opened = false;
        self.pending_window = false;
        self.released = false;
        self.hash.clear();
        self.stop_saver();
        for view in &self.views {
            view.set_message("".into());
            let _ = view.hide();
        }
        self.restore.take()
    }
}
