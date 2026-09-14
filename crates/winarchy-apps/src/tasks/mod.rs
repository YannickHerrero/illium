//! Task manager: one list of processes, refreshed every two seconds.
pub mod model;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub fn run() -> Result<(), String> {
    use crate::{
        key::Key,
        ui::{self, TaskRow, TasksWindow},
    };
    use model::{Action, Mode, Tasks};
    use slint::{ComponentHandle, Model, ModelRc, VecModel};
    use std::{cell::RefCell, rc::Rc};
    let window = TasksWindow::new().map_err(|e| e.to_string())?;
    ui::apply(
        window.global::<ui::Palette>(),
        &winarchy_theme::Theme::current(&ui::config_home()),
    );
    window.set_help_lines(ui::strings(&model::HELP));
    let rows = Rc::new(VecModel::<TaskRow>::default());
    window.set_rows(ModelRc::from(rows.clone()));
    let tasks = Rc::new(RefCell::new(Tasks::default()));
    let render = {
        let window = window.as_weak();
        let rows = rows.clone();
        move |t: &Tasks| {
            let Some(window) = window.upgrade() else {
                return;
            };
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
    };
    let render = Rc::new(render);
    {
        let tasks = tasks.clone();
        let render = render.clone();
        let weak = window.as_weak();
        window.on_key(move |text, _ctrl, _shift| {
            let Some(key) = Key::from_slint(&text) else {
                return;
            };
            let Some(window) = weak.upgrade() else { return };
            let mut t = tasks.borrow_mut();
            match t.key(key, window.get_page().max(1) as usize) {
                Action::Quit => {
                    let _ = slint::quit_event_loop();
                }
                Action::Kill(pid) => {
                    if let Err(e) = win::terminate(pid) {
                        t.notice = format!("cannot end {pid}: {e}");
                    }
                }
                Action::None => {}
            }
            render(&t);
        });
    }
    // Sampling opens every process handle; it stays off the UI thread and
    // hands each snapshot over a channel the UI polls.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut sampler = win::Sampler::default();
        while tx.send(sampler.sample()).is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    let timer = slint::Timer::default();
    {
        let tasks = tasks.clone();
        let render = render.clone();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(250),
            move || {
                if let Some(processes) = rx.try_iter().last() {
                    let mut t = tasks.borrow_mut();
                    t.update(processes);
                    render(&t);
                }
            },
        );
    }
    window.run().map_err(|e| e.to_string())
}
