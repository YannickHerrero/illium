//! Passive toasts in the top-right corner of the active monitor.
use super::{NotificationItem, NotificationsView, color, id, tool};
use crate::{
    config::Config,
    layout::Rect,
    notifications::Stack,
    platform::{Event, EventSender, dpi, native},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, HWND_TOPMOST};

const WIDTH: i32 = 360;
const ITEM_HEIGHT: i32 = 60;
const SPACING: i32 = 8;
const MARGIN: i32 = 12;
pub struct Notifications {
    ui: NotificationsView,
    stack: Stack,
    /// Top-right corner the stack hangs from, in physical pixels.
    anchor: Option<(Rect, i32)>,
    pending: Option<Rect>,
    restore: Option<isize>,
    shown: bool,
}
impl Notifications {
    pub fn new(tx: EventSender) -> Result<Self, String> {
        let ui = NotificationsView::new().map_err(|e| e.to_string())?;
        ui.on_dismiss(move |n| {
            let _ = tx.send(Event::DismissNotification(n as u64));
        });
        Ok(Self {
            ui,
            stack: Stack::default(),
            anchor: None,
            pending: None,
            restore: None,
            shown: false,
        })
    }
    pub fn prewarm(&self) {
        if !self.shown {
            super::prewarm(&self.ui);
        }
    }
    pub fn apply_theme(&self, c: &Config) {
        self.ui.set_bg(color(&c.theme.surface));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_muted(color(&c.theme.subtext));
        self.ui.set_accent(color(&c.theme.accent));
        self.ui.set_overlay(color(&c.theme.overlay));
    }
    pub fn push(&mut self, c: &Config, monitor: Rect, title: &str, body: &str) {
        let timeout = Duration::from_secs(c.global.notifications.timeout);
        self.stack.push(title, body, Instant::now(), timeout);
        let gap = dpi::scale(monitor, 6);
        let top = if c.bar.position == "top" {
            monitor.y + dpi::scale(monitor, c.bar.height) + gap
        } else {
            monitor.y + gap
        };
        self.anchor = Some((monitor, top));
        self.refresh();
    }
    pub fn dismiss(&mut self, id: u64) {
        if self.stack.dismiss(id) {
            self.refresh();
        }
    }
    pub fn poll(&mut self) {
        if self.stack.expire(Instant::now()) {
            self.refresh();
        }
    }
    fn refresh(&mut self) {
        let items = self.stack.items();
        let Some((monitor, top)) = self.anchor.filter(|_| !items.is_empty()) else {
            self.pending = None;
            self.shown = false;
            let _ = self.ui.hide();
            return;
        };
        let count = items.len() as i32;
        let logical_height = count * ITEM_HEIGHT + (count - 1) * SPACING;
        let w = dpi::scale(monitor, WIDTH);
        let r = Rect {
            x: monitor.x + monitor.w - w - dpi::scale(monitor, MARGIN),
            y: top,
            w,
            h: dpi::scale(monitor, logical_height),
        };
        self.ui.set_items(ModelRc::new(VecModel::from(
            items
                .iter()
                .map(|n| NotificationItem {
                    id: n.id as i32,
                    title: n.title.as_str().into(),
                    body: n.body.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )));
        self.ui.set_item_height(ITEM_HEIGHT as f32);
        self.ui.set_spacing(SPACING as f32);
        self.ui.set_surface_width(dpi::logical(monitor, w));
        self.ui.set_surface_height(dpi::logical(monitor, r.h));
        if !self.shown {
            self.restore = Some(unsafe { GetForegroundWindow().0 as isize });
        }
        super::prepare(self.ui.window(), r, true);
        if let Err(e) = self.ui.show() {
            tracing::warn!(%e, "notification unavailable");
            return;
        }
        self.shown = true;
        self.pending = Some(r);
    }
    pub fn arrange(&mut self) {
        let Some(r) = self.pending else {
            return;
        };
        let hwnd = id(self.ui.window());
        if hwnd == 0 {
            return;
        }
        tool(self.ui.window(), true);
        native::position(hwnd, r, Some(HWND_TOPMOST));
        // winit may activate a newly created HWND before it is marked NOACTIVATE.
        if unsafe { GetForegroundWindow().0 as isize } == hwnd
            && let Some(restore) = self
                .restore
                .filter(|w| native::visible(*w) && !native::minimized(*w))
        {
            native::focus(restore, false);
        }
        self.pending = None;
    }
}
