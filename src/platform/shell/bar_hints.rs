//! Passive hint strip: real Slint module centers, frozen until selection ends.
use super::{BarHint, BarHints, color, id, tool};
use crate::{
    bar_hints::{self, Input},
    config::Config,
    layout::Rect,
};
use slint::{ComponentHandle, ModelRc, VecModel};
use windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST;

pub struct Hints {
    ui: BarHints,
    pub opened: bool,
    pub generation: u32,
    monitor: usize,
    targets: Vec<(String, Option<f32>)>,
    selected: usize,
    pending: Option<Rect>,
    ready: bool,
}
impl Hints {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            ui: BarHints::new().map_err(|e| e.to_string())?,
            opened: false,
            generation: 0,
            monitor: 0,
            targets: vec![],
            selected: 0,
            pending: None,
            ready: false,
        })
    }
    pub fn prewarm(&self) { if !self.opened { super::prewarm(self.ui.window()); } }
    pub fn open(&mut self, c: &Config, r: Rect, monitor: usize, kinds: Vec<String>) {
        self.close();
        if kinds.is_empty() {
            return;
        }
        self.generation = self.generation % i32::MAX as u32 + 1;
        self.monitor = monitor;
        self.targets = kinds
            .into_iter()
            .take(bar_hints::LABELS.len())
            .map(|k| (k, None))
            .collect();
        self.selected = 0;
        self.ui.set_selected(0);
        self.ui.set_bg(color(&c.theme.surface));
        self.ui.set_fg(color(&c.theme.text));
        self.ui.set_accent(color(&c.theme.accent));
        self.ui
            .set_surface_width(super::super::dpi::logical(r, r.w));
        let bar = super::super::dpi::scale(r, c.bar.height);
        let gap = super::super::dpi::scale(r, 4);
        let height = super::super::dpi::scale(r, 26);
        self.pending = Some(Rect {
            x: r.x,
            y: if c.bar.position == "top" {
                r.y + bar + gap
            } else {
                r.y + r.h - bar - gap - height
            },
            w: r.w,
            h: height,
        });
        self.opened = true;
    }
    pub fn position(&mut self, generation: u32, index: usize, x: f32) {
        if !self.opened || generation != self.generation || self.ready {
            return;
        }
        let Some((_, center)) = self.targets.get_mut(index) else {
            return;
        };
        *center = Some(x);
        if self.targets.iter().any(|(_, x)| x.is_none()) {
            return;
        }
        // Actual geometry, not configuration group order, defines left-to-right.
        self.targets
            .sort_by(|a, b| a.1.unwrap().total_cmp(&b.1.unwrap()));
        self.ui.set_hints(ModelRc::new(VecModel::from(
            self.targets
                .iter()
                .enumerate()
                .map(|(i, (_, x))| BarHint {
                    center: x.unwrap(),
                    label: bar_hints::label(i).unwrap().to_string().into(),
                })
                .collect::<Vec<_>>(),
        )));
        if let Some(r) = self.pending { super::prepare(self.ui.window(), r, true); }
        if let Err(e) = self.ui.show() {
            tracing::warn!(%e, "bar hints unavailable");
            self.close();
            return;
        }
        self.ready = true;
    }
    pub fn arrange(&mut self, restore: Option<isize>) {
        if !self.ready {
            return;
        }
        if let Some(r) = self.pending
            && id(self.ui.window()) != 0
        {
            tool(self.ui.window(), true);
            super::super::native::position(id(self.ui.window()), r, Some(HWND_TOPMOST));
            // winit may activate a newly created HWND before we can mark it
            // NOACTIVATE. Keep selection passive without stealing a later focus change.
            let foreground = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize
            };
            if foreground == id(self.ui.window())
                && let Some(restore) = restore.filter(|id| {
                    super::super::native::visible(*id) && !super::super::native::minimized(*id)
                })
            {
                super::super::native::focus(restore, false);
            }
            self.pending = None;
        }
    }
    pub fn input(&mut self, generation: u32, input: Input) -> Option<(String, i32, usize)> {
        if !self.opened || generation != self.generation {
            return None;
        }
        if input == Input::Cancel {
            self.close();
            return None;
        }
        if !self.ready {
            return None;
        }
        let select = match input {
            Input::Select(i) => Some(i),
            Input::Accept => Some(self.selected),
            Input::Previous | Input::Next => {
                self.selected =
                    bar_hints::step(self.selected, self.targets.len(), input == Input::Next);
                self.ui.set_selected(self.selected as i32);
                None
            }
            Input::Cancel => None,
        }?;
        let (kind, x) = self.targets.get(select)?;
        let target = (kind.clone(), x.unwrap() as i32, self.monitor);
        self.close();
        Some(target)
    }
    pub fn close(&mut self) {
        self.opened = false;
        self.ready = false;
        self.pending = None;
        self.targets.clear();
        let _ = self.ui.hide();
    }
}
