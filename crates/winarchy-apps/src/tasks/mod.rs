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
        sync::{Arc, Mutex, Condvar},
        time::Duration,
    };
    #[derive(Default)]
    struct Sampling { visible: bool, generation: u64, stopped: bool }
    pub struct App {
        window: TasksWindow,
        _theme: winarchy_theme::live::Subscription,
        /// The sampler only works while the window is shown.
        sampling: Arc<(Mutex<Sampling>, Condvar)>,
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
            window.set_status_left("Reading processes…".into());
            let sampling = Arc::new((Mutex::new(Sampling::default()), Condvar::new()));
            {
                let tasks = tasks.clone();
                let rows = rows.clone();
                let weak = window.as_weak();
                let sampling = sampling.clone();
                window.on_key(move |text, _ctrl, _shift| {
                    let Some(key) = Key::from_slint(&text) else {
                        return;
                    };
                    let Some(window) = weak.upgrade() else { return };
                    let mut t = tasks.borrow_mut();
                    match t.key(key, window.get_page().max(1) as usize) {
                        Action::Quit => {
                            if resident {
                                sampling.0.lock().unwrap().visible = false;
                                sampling.1.notify_one();
                                let _ = window.hide();
                            } else {
                                let _ = slint::quit_event_loop();
                            }
                        }
                        Action::Kill(pid) => {
                            match win::terminate(pid) {
                                Err(e) => t.notice = format!("cannot end {pid}: {e}"),
                                Ok(()) => {
                                    // Acknowledge the request, not an unconfirmed exit.
                                    t.notice = format!("Ending process {pid}…");
                                    let mut state = sampling.0.lock().unwrap();
                                    state.generation = state.generation.wrapping_add(1);
                                    sampling.1.notify_one();
                                }
                            }
                        }
                        Action::None => {}
                    }
                    render(&window, &rows, &t);
                });
            }
            // One bounded result and a real UI wakeup, with no 250ms polling.
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            {
                let weak = window.as_weak();
                window.on_sample_ready(move || {
                    if let Ok(processes) = rx.try_recv() && let Some(window) = weak.upgrade() {
                        let mut t = tasks.borrow_mut();
                        t.update(processes);
                        render(&window, &rows, &t);
                    }
                });
            }
            {
                let sampling = sampling.clone();
                let weak = window.as_weak();
                std::thread::spawn(move || {
                    let mut sampler = win::Sampler::default();
                    let mut last_generation = None;
                    loop {
                        let state = sampling.0.lock().unwrap();
                        let state = sampling.1.wait_while(state, |s| !s.visible && !s.stopped).unwrap();
                        if state.stopped { return; }
                        let generation = state.generation;
                        let first = last_generation != Some(generation);
                        last_generation = Some(generation);
                        drop(state);
                        if tx.send(sampler.sample()).is_err() { return; }
                        if weak.upgrade_in_event_loop(|window| window.invoke_sample_ready()).is_err() { return; }
                        let state = sampling.0.lock().unwrap();
                        let delay = if first { Duration::from_millis(300) } else { Duration::from_secs(2) };
                        let _ = sampling.1.wait_timeout_while(state, delay, |s| {
                            s.visible && !s.stopped && s.generation == generation
                        }).unwrap();
                    }
                });
            }
            let theme = ui::watch_theme(&window)?;
            Ok(Self {
                window,
                _theme: theme,
                sampling,
            })
        }
        pub fn show(&self) -> Result<(), String> {
            {
                let mut state = self.sampling.0.lock().unwrap();
                state.visible = true;
                state.generation = state.generation.wrapping_add(1);
            }
            self.sampling.1.notify_one();
            self.window.show().map_err(|e| e.to_string())?;
            // A window shown again after hide() keeps its last frame; ask for a
            // fresh one so a stale or empty surface never stays on screen.
            self.window.window().request_redraw();
            ui::raise(&self.window);
            Ok(())
        }
    }
    impl Drop for App {
        fn drop(&mut self) {
            self.sampling.0.lock().unwrap().stopped = true;
            self.sampling.1.notify_one();
        }
    }
}
#[cfg(windows)]
pub fn run() -> Result<(), String> {
    let app = App::new(false)?;
    app.show()?;
    slint::run_event_loop().map_err(|e| e.to_string())
}
