mod input;
pub mod ipc;
mod native;
mod session;
mod shell;
use crate::{
    command::Command,
    config::Config,
    layout::{Rect, fibonacci, neighbor},
    model::{Client, Model},
};
pub use session::watchdog;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{self, Sender},
};
use windows::Win32::{
    Foundation::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};
pub enum Event {
    Command(Command, Option<Sender<Result<String, String>>>),
    Window(u32, isize),
    Search(String),
    Launch(i32),
    Dismiss,
    Reload,
}
struct Manager {
    config: Config,
    model: Model,
    monitors: Vec<Rect>,
    shell: shell::Shell,
}
impl Manager {
    fn add(&mut self, id: isize) {
        if self.model.clients.iter().any(|c| c.id == id) {
            return;
        }
        let Some((exe, class, mut floating)) = native::metadata(id) else {
            return;
        };
        let title = native::title(id);
        let mut workspace = self.model.active;
        for r in &self.config.rules.rules {
            if r.matches(&exe, &class, &title) {
                if r.ignore {
                    return;
                }
                floating |= r.floating;
                workspace = r.workspace.unwrap_or(workspace);
            }
        }
        self.model.clients.push(Client {
            id,
            workspace,
            floating,
            fullscreen: false,
            restore: native::rect(id),
        });
        tracing::info!(id,workspace,%title,"window added");
    }
    fn area(&self) -> Rect {
        let index = self.model.monitors[(self.model.active - 1) as usize]
            .min(self.monitors.len().saturating_sub(1));
        let mut r = self.monitors.get(index).copied().unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 1280,
            h: 720,
        });
        if self.config.bar.enabled {
            r.h -= self.config.bar.height;
            if self.config.bar.position == "top" {
                r.y += self.config.bar.height;
            }
        }
        r
    }
    fn layout(&mut self) {
        self.model
            .clients
            .retain(|c| unsafe { IsWindow(Some(native::hwnd(c.id))).as_bool() });
        for c in &self.model.clients {
            let visible = c.workspace == self.model.active;
            let currently = unsafe { IsWindowVisible(native::hwnd(c.id)).as_bool() };
            if visible != currently {
                native::show(c.id, visible);
            }
        }
        let area = self.area();
        let ids: Vec<_> = self
            .model
            .clients
            .iter()
            .filter(|c| c.workspace == self.model.active && !c.floating && !c.fullscreen)
            .map(|c| c.id)
            .collect();
        let rs = fibonacci(
            area,
            ids.len(),
            self.config.wm.gap,
            self.config.wm.outer_gap,
        );
        native::batch(&ids.into_iter().zip(rs).collect::<Vec<_>>());
        for c in &self.model.clients {
            if c.workspace == self.model.active && c.fullscreen {
                native::position(c.id, area, Some(HWND_TOP));
            }
        }
        self.shell.refresh(&self.model, &self.config);
    }
    fn focus_visible(&mut self) {
        let id = self
            .model
            .focused
            .filter(|id| {
                self.model
                    .clients
                    .iter()
                    .any(|c| c.id == *id && c.workspace == self.model.active)
            })
            .or_else(|| {
                self.model
                    .clients
                    .iter()
                    .find(|c| c.workspace == self.model.active)
                    .map(|c| c.id)
            });
        self.model.focused = id;
        if let Some(id) = id {
            native::focus(id);
        }
    }
    fn reload(&mut self) -> Result<(), String> {
        let config = Config::load(&self.config.home)?;
        let bindings = input::parse(&config.keys)?;
        self.shell.configure(&config, &self.monitors)?;
        input::update(bindings);
        self.config = config;
        self.layout();
        tracing::info!("configuration reloaded");
        Ok(())
    }
    fn execute(&mut self, c: Command) -> Result<String, String> {
        tracing::debug!(?c, "command");
        match c {
            Command::Workspace(n) => {
                self.model.switch(n);
                self.layout();
                self.focus_visible();
            }
            Command::Next => {
                self.model.switch(self.model.next());
                self.layout();
                self.focus_visible();
            }
            Command::Recent => {
                self.model.switch(self.model.recent);
                self.layout();
                self.focus_visible();
            }
            Command::MoveWorkspace(n, follow) => {
                if let Some(id) = self.model.focused {
                    self.model.move_to(id, n, follow);
                    self.layout();
                    self.focus_visible();
                }
            }
            Command::Close => {
                if let Some(id) = self.model.focused {
                    native::close(id);
                }
            }
            Command::Focus(d) | Command::Move(d) => {
                if let Some(id) = self.model.focused {
                    let rs = self
                        .model
                        .clients
                        .iter()
                        .filter(|w| {
                            w.workspace == self.model.active
                                && (!matches!(c, Command::Move(_)) || !w.floating)
                        })
                        .map(|w| (w.id, native::rect(w.id)))
                        .collect::<Vec<_>>();
                    if let Some(other) = neighbor(&rs, id, d) {
                        if matches!(c, Command::Move(_)) {
                            self.model.swap(id, other);
                            self.layout();
                        } else {
                            self.model.focused = Some(other);
                            native::focus(other);
                        }
                    }
                }
            }
            Command::Tile | Command::Float | Command::Fullscreen => {
                let area = self.area();
                if let Some(w) = self
                    .model
                    .clients
                    .iter_mut()
                    .find(|w| Some(w.id) == self.model.focused)
                {
                    match c {
                        Command::Tile => {
                            w.floating = false;
                            w.fullscreen = false;
                        }
                        Command::Float => {
                            w.fullscreen = false;
                            w.floating = !w.floating;
                            if w.floating {
                                let r = Rect {
                                    x: area.x + area.w / 6,
                                    y: area.y + area.h / 6,
                                    w: area.w * 2 / 3,
                                    h: area.h * 2 / 3,
                                };
                                w.restore = r;
                                native::position(w.id, r, None);
                            }
                        }
                        _ => {
                            if !w.fullscreen {
                                w.restore = native::rect(w.id);
                            }
                            w.fullscreen = !w.fullscreen;
                            if !w.fullscreen {
                                native::position(w.id, w.restore, None);
                            }
                        }
                    }
                    self.layout();
                }
            }
            Command::Spawn(app) => {
                let target = self
                    .config
                    .apps
                    .apps
                    .get(&app)
                    .ok_or_else(|| format!("unknown application: {app}"))?;
                native::spawn(target)?;
            }
            Command::Launcher => {
                self.shell.toggle(&self.config, self.area())?;
            }
            Command::Reload => self.reload()?,
            Command::Theme(name) => {
                let path = self.config.home.join("winarchy.toml");
                let old = std::fs::read(&path).map_err(|e| e.to_string())?;
                std::fs::write(&path, format!("theme = {name:?}\n")).map_err(|e| e.to_string())?;
                if let Err(e) = self.reload() {
                    let _ = std::fs::write(path, old);
                    return Err(e);
                }
            }
            Command::Explorer(start) => session::explorer(start)?,
            Command::Quit => {
                slint::quit_event_loop().map_err(|e| e.to_string())?;
            }
        }
        Ok("ok".into())
    }
    fn event(&mut self, event: Event) {
        match event {
            Event::Command(c, reply) => {
                let result = self.execute(c);
                if let Err(e) = &result {
                    tracing::warn!(%e,"command failed");
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            Event::Window(event, id) => match event {
                EVENT_OBJECT_DESTROY => {
                    self.model.clients.retain(|c| c.id != id);
                    tracing::info!(id, "window removed");
                    self.layout();
                }
                EVENT_OBJECT_HIDE => {
                    if self
                        .model
                        .clients
                        .iter()
                        .any(|c| c.id == id && c.workspace == self.model.active)
                        && !unsafe { IsWindowVisible(native::hwnd(id)).as_bool() }
                    {
                        self.model.clients.retain(|c| c.id != id);
                        self.layout();
                    }
                }
                EVENT_SYSTEM_FOREGROUND => {
                    if let Some(c) = self.model.clients.iter().find(|c| c.id == id) {
                        if c.workspace == self.model.active {
                            self.model.focused = Some(id);
                            let r = native::rect(id);
                            if let Some(index) = self.monitors.iter().position(|m| {
                                r.x + r.w / 2 >= m.x
                                    && r.x + r.w / 2 < m.x + m.w
                                    && r.y + r.h / 2 >= m.y
                                    && r.y + r.h / 2 < m.y + m.h
                            }) {
                                self.model.monitors[(self.model.active - 1) as usize] = index;
                            }
                        }
                    }
                    self.shell.refresh(&self.model, &self.config);
                }
                EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW => {
                    self.add(id);
                    self.layout();
                }
                EVENT_SYSTEM_MOVESIZEEND => self.layout(),
                _ => {}
            },
            Event::Search(q) => self.shell.search(&q, self.config.launcher.max_results),
            Event::Launch(n) => {
                if let Some(app) = self.shell.results.get(n.max(0) as usize).cloned() {
                    self.shell.dismiss();
                    let result = if app.shortcut {
                        native::shortcut(&app.target)
                    } else {
                        native::spawn(&app.target)
                    };
                    if let Err(e) = result {
                        tracing::error!(%e,"launch failed");
                    }
                }
            }
            Event::Dismiss => {
                self.shell.dismiss();
                self.focus_visible();
            }
            Event::Reload => {
                if let Err(e) = self.reload() {
                    tracing::warn!(%e,"keeping previous configuration");
                }
            }
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        for c in &self.model.clients {
            native::show(c.id, true);
            if c.fullscreen {
                native::position(c.id, c.restore, None);
            }
        }
    }
}
fn watch(home: std::path::PathBuf, tx: Sender<Event>) {
    std::thread::spawn(move || unsafe {
        use windows::{
            Win32::{Storage::FileSystem::*, System::Threading::*},
            core::PCWSTR,
        };
        let path = native::wide(&home.to_string_lossy());
        let Ok(h) = FindFirstChangeNotificationW(
            PCWSTR(path.as_ptr()),
            true,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
        ) else {
            return;
        };
        loop {
            if WaitForSingleObject(h, INFINITE) != WAIT_OBJECT_0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = tx.send(Event::Reload);
            if FindNextChangeNotification(h).is_err() {
                break;
            }
        }
        let _ = FindCloseChangeNotification(h);
    });
}
pub fn run(replace: bool) -> Result<(), String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let home = Config::home();
    Config::install(&home)?;
    let config = Config::load(&home)?;
    let bindings = input::parse(&config.keys)?;
    let (tx, rx) = mpsc::channel();
    ipc::start(tx.clone())?;
    let mut manager = Manager {
        config,
        model: Model::new(),
        monitors: native::monitors(),
        shell: shell::Shell::new(tx.clone())?,
    };
    manager
        .shell
        .configure(&manager.config, &manager.monitors)?;
    for id in native::enumerate() {
        manager.add(id);
    }
    manager.model.focused = Some(unsafe { GetForegroundWindow().0 as isize });
    manager.layout();
    input::start(tx.clone(), bindings)?;
    watch(home, tx);
    let manager = Rc::new(RefCell::new(manager));
    let m = manager.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(10),
        move || {
            let mut m = m.borrow_mut();
            for event in rx.try_iter().take(128) {
                m.event(event);
            }
        },
    );
    let m = manager.clone();
    let status = slint::Timer::default();
    status.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            let mut m = m.borrow_mut();
            let monitors = native::monitors();
            if monitors != m.monitors {
                m.monitors = monitors;
                let c = m.config.clone();
                let monitors = m.monitors.clone();
                let _ = m.shell.configure(&c, &monitors);
                m.layout();
            }
            m.shell.refresh(&m.model, &m.config);
        },
    );
    let _guard = session::Recovery::new(replace)?;
    tracing::info!("Winarchy ready");
    slint::run_event_loop().map_err(|e| e.to_string())
}
