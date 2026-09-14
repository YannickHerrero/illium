//! Task manager: one list of processes, refreshed every two seconds.
pub mod model;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use app::App;
#[cfg(windows)]
mod app {
    use super::{
        model::{self, Action, Mode, Tasks},
        win,
    };
    use crate::{
        key::Key,
        ui::{self, TaskRow, TasksWindow},
    };
    use slint::{ComponentHandle, Model, ModelRc, VecModel};
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    pub struct App {
        window: TasksWindow,
        /// The sampler only works while the window is shown.
        visible: Arc<AtomicBool>,
        /// The native window exists after the first show; later shows reuse it.
        shown: std::cell::Cell<bool>,
        _timer: slint::Timer,
    }
    fn render(window: &TasksWindow, rows: &VecModel<TaskRow>, t: &Tasks) {
        let fresh: Vec<TaskRow> = t
            .rows()
            .iter()
            .map(|p| TaskRow {
                pid: p.pid as i32,
                name: p.name.as_str().into(),
                cpu: format!("{:.1}%", p.cpu).into(),
                memory: model::memory(p.memory).into(),
                accessible: p.accessible,
            })
            .collect();
        for (i, row) in fresh.iter().enumerate() {
            match rows.row_data(i) {
                Some(current) if current == *row => {}
                Some(_) => rows.set_row_data(i, row.clone()),
                None => rows.push(row.clone()),
            }
        }
        while rows.row_count() > fresh.len() {
            rows.remove(rows.row_count() - 1);
        }
        window.set_cursor(t.cursor as i32);
        let (left, right) = t.status();
        window.set_status_left(left.into());
        window.set_status_right(right.into());
        window.set_help(t.mode == Mode::Help);
    }
    impl App {
        pub fn new(resident: bool) -> Result<Self, String> {
            let window = TasksWindow::new().map_err(|e| e.to_string())?;
            window.set_help_lines(ui::strings(&model::HELP));
            let rows = Rc::new(VecModel::<TaskRow>::default());
            window.set_rows(ModelRc::from(rows.clone()));
            let tasks = Rc::new(RefCell::new(Tasks::default()));
            let visible = Arc::new(AtomicBool::new(false));
            {
                let tasks = tasks.clone();
                let rows = rows.clone();
                let weak = window.as_weak();
                let visible = visible.clone();
                window.on_key(move |text, _ctrl, _shift| {
                    let Some(key) = Key::from_slint(&text) else {
                        return;
                    };
                    let Some(window) = weak.upgrade() else { return };
                    let mut t = tasks.borrow_mut();
                    match t.key(key, window.get_page().max(1) as usize) {
                        Action::Quit => {
                            if resident {
                                visible.store(false, Ordering::Relaxed);
                                ui::set_visible(&window, false);
                            } else {
                                let _ = slint::quit_event_loop();
                            }
                        }
                        Action::Kill(pid) => {
                            if let Err(e) = win::terminate(pid) {
                                t.notice = format!("cannot end {pid}: {e}");
                            }
                        }
                        Action::None => {}
                    }
                    render(&window, &rows, &t);
                });
            }
            // Sampling opens every process handle; it stays off the UI thread and
            // hands each snapshot over a channel the UI polls.
            let (tx, rx) = std::sync::mpsc::channel();
            {
                let visible = visible.clone();
                std::thread::spawn(move || {
                    let mut sampler = win::Sampler::default();
                    loop {
                        if visible.load(Ordering::Relaxed) && tx.send(sampler.sample()).is_err() {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(
                            if visible.load(Ordering::Relaxed) {
                                2000
                            } else {
                                250
                            },
                        ));
                    }
                });
            }
            let timer = slint::Timer::default();
            {
                let weak = window.as_weak();
                timer.start(
                    slint::TimerMode::Repeated,
                    std::time::Duration::from_millis(250),
                    move || {
                        if let Some(processes) = rx.try_iter().last()
                            && let Some(window) = weak.upgrade()
                        {
                            let mut t = tasks.borrow_mut();
                            t.update(processes);
                            render(&window, &rows, &t);
                        }
                    },
                );
            }
            Ok(Self {
                window,
                visible,
                shown: std::cell::Cell::new(false),
                _timer: timer,
            })
        }
        pub fn show(&self) -> Result<(), String> {
            ui::apply(
                self.window.global::<ui::Palette>(),
                &winarchy_theme::Theme::current(&ui::config_home()),
            );
            self.visible.store(true, Ordering::Relaxed);
            if self.shown.replace(true) {
                ui::set_visible(&self.window, true);
            } else {
                self.window.show().map_err(|e| e.to_string())?;
            }
            ui::raise(&self.window);
            Ok(())
        }
    }
}
#[cfg(windows)]
pub fn run() -> Result<(), String> {
    let app = App::new(false)?;
    app.show()?;
    slint::run_event_loop().map_err(|e| e.to_string())
}
