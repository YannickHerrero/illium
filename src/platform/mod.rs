mod dpi;
mod input;
mod instance;
pub mod ipc;
mod native;
mod pipe_io;
#[cfg(test)]
mod pipe_io_tests;
mod security;
mod session;
mod shell;
mod status;
use crate::{
    command::Command,
    config::Config,
    layout::{Rect, fibonacci, neighbor},
    model::{Client, Model},
};
pub use security::require_standard_user;
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
    Command(Command, Option<crate::request::ReplyTo>),
    Window(u32, isize),
    Search(String),
    Launch(i32),
    Dismiss,
    Reload,
    Display,
    Mouse(isize),
}
struct Manager {
    config: Config,
    model: Model,
    monitors: Vec<Rect>,
    shell: shell::Shell,
}
impl Manager {
    fn add(&mut self, id: isize) -> bool {
        if let Some(c) = self.model.clients.iter().find(|c| c.id == id) {
            if c.workspace != self.model.active {
                native::show(id, false);
            }
            return false;
        }
        let Some((exe, class, mut floating)) = native::metadata(id) else {
            return false;
        };
        let title = native::title(id);
        let mut workspace = self.model.active;
        for r in &self.config.rules.rules {
            if r.matches(&exe, &class, &title) {
                if r.ignore {
                    return false;
                }
                floating |= r.floating;
                workspace = r.workspace.unwrap_or(workspace);
            }
        }
        session::tag(id);
        self.model.clients.push(Client {
            id,
            workspace,
            floating,
            fullscreen: false,
            restore: native::rect(id),
        });
        tracing::info!(id, workspace, "window added");
        true
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
            let height = dpi::scale(r, self.config.bar.height);
            r.h -= height;
            if self.config.bar.position == "top" {
                r.y += height;
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
            dpi::scale(area, self.config.wm.gap),
            dpi::scale(area, self.config.wm.outer_gap),
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
        match &c {
            Command::Spawn(_) | Command::LaunchTarget { .. } => {
                tracing::debug!("application launch command")
            }
            _ => tracing::debug!(?c, "command"),
        }
        let foreground = unsafe { GetForegroundWindow().0 as isize };
        if self
            .model
            .clients
            .iter()
            .any(|w| w.id == foreground && w.workspace == self.model.active)
        {
            self.model.focused = Some(foreground);
        }
        match c {
            Command::Status => return Ok(serde_json::json!({
                "workspace": self.model.active, "recent": self.model.recent,
                "focused": self.model.focused, "theme": self.config.global.theme,
                "gap": self.config.wm.gap, "launcher": self.shell.visible,
                "monitors": self.monitors, "bar_count": self.shell.bars.len(),
                "clients": self.model.clients.iter().map(|c| serde_json::json!({"id":c.id,"workspace":c.workspace,"floating":c.floating,"fullscreen":c.fullscreen,"title":native::title(c.id),"rect":native::rect(c.id)})).collect::<Vec<_>>()
            }).to_string()),
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
                    .ok_or_else(|| format!("unknown application: {app}"))?.clone();
                return self.execute(Command::LaunchTarget { target, shortcut: false });
            }
            Command::LaunchTarget { target, shortcut } => {
                if shortcut { native::shortcut(&target)?; } else { native::spawn(&target)?; }
            }
            Command::Launcher => {
                self.shell.toggle(&self.config, self.area())?;
            }
            Command::Reload => self.reload()?,
            Command::Theme(name) => {
                let path = self.config.home.join("winarchy.toml");
                let old = crate::files::read_config(&path)?;
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
                if let Some(reply) = &reply
                    && !reply.ticket.start(std::time::Instant::now())
                {
                    let _ = reply
                        .sender
                        .send(Err("IPC request expired before execution".into()));
                    return;
                }
                let result = self.execute(c);
                if let Err(e) = &result {
                    tracing::warn!(%e,"command failed");
                }
                if let Some(reply) = reply {
                    let _ = reply.sender.send(result);
                }
            }
            Event::Window(event, id) => match event {
                EVENT_OBJECT_DESTROY => {
                    if self.model.clients.iter().any(|c| c.id == id) {
                        self.model.clients.retain(|c| c.id != id);
                        tracing::info!(id, "window removed");
                        self.layout();
                    }
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
                        session::untag(id);
                        self.layout();
                    }
                }
                EVENT_SYSTEM_FOREGROUND => {
                    if let Some(c) = self.model.clients.iter().find(|c| c.id == id)
                        && c.workspace == self.model.active
                    {
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
                    self.shell.refresh(&self.model, &self.config);
                }
                EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW => {
                    if self.add(id) {
                        self.layout();
                    }
                }
                EVENT_SYSTEM_MOVESIZEEND => self.layout(),
                _ => {}
            },
            Event::Search(q) => self.shell.search(&q, self.config.launcher.max_results),
            Event::Launch(n) => {
                if let Some(app) = self.shell.results.get(n.max(0) as usize).cloned() {
                    self.shell.dismiss();
                    let result = self.execute(Command::LaunchTarget {
                        target: app.target,
                        shortcut: app.shortcut,
                    });
                    if let Err(e) = result {
                        tracing::error!(%e,"launch failed");
                    }
                }
            }
            Event::Dismiss => {
                self.shell.dismiss();
                self.focus_visible();
            }
            Event::Mouse(id) => {
                if self.config.wm.focus_follows_mouse
                    && !self.shell.visible
                    && self
                        .model
                        .clients
                        .iter()
                        .any(|c| c.id == id && c.workspace == self.model.active)
                {
                    self.model.focused = Some(id);
                    native::focus(id);
                }
            }
            Event::Display => {
                let monitors = native::monitors();
                if !monitors.is_empty() && monitors != self.monitors {
                    tracing::info!(count = monitors.len(), "display configuration changed");
                    self.monitors = monitors;
                    for index in &mut self.model.monitors {
                        *index = (*index).min(self.monitors.len() - 1);
                    }
                    let _ = self.shell.configure(&self.config, &self.monitors);
                    self.layout();
                }
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
            session::untag(c.id);
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
        let mut previous = crate::files::snapshot(&home);
        loop {
            if WaitForSingleObject(h, INFINITE) != WAIT_OBJECT_0 {
                break;
            }
            if FindNextChangeNotification(h).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            let next = crate::files::snapshot(&home);
            if next != previous {
                previous = next;
                let _ = tx.send(Event::Reload);
            }
        }
        let _ = FindCloseChangeNotification(h);
    });
}
pub fn run(replace: bool) -> Result<(), String> {
    security::require_standard_user()?;
    let _instance = instance::Instance::acquire()?;
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
    let home = Config::home();
    Config::install(&home)?;
    let config = Config::load(&home)?;
    let bindings = input::parse(&config.keys)?;
    let (tx, rx) = mpsc::channel();
    ipc::start(tx.clone())?;
    let manager = Manager {
        config,
        model: Model::new(),
        monitors: native::monitors(),
        shell: shell::Shell::new(tx.clone())?,
    };
    let manager = Rc::new(RefCell::new(manager));
    let startup_error = Rc::new(RefCell::new(None));
    let recovery = Rc::new(RefCell::new(None));
    let m = manager.clone();
    let error = startup_error.clone();
    let startup_recovery = recovery.clone();
    slint::Timer::single_shot(std::time::Duration::from_millis(1), move || {
        let result = (|| -> Result<(), String> {
            // Arm crash recovery before any client can be tagged or hidden.
            *startup_recovery.borrow_mut() = Some(session::Recovery::new()?);
            let mut m = m.borrow_mut();
            let config = m.config.clone();
            let monitors = m.monitors.clone();
            m.shell.configure(&config, &monitors)?;
            for id in native::enumerate() {
                m.add(id);
            }
            let foreground = unsafe { GetForegroundWindow().0 as isize };
            m.model.focused = m
                .model
                .clients
                .iter()
                .find(|c| c.id == foreground)
                .map(|c| c.id);
            m.layout();
            m.focus_visible();
            input::start(tx.clone(), bindings)?;
            watch(home, tx);
            tracing::info!("Winarchy core initialized");
            Ok(())
        })();
        if let Err(e) = result {
            *error.borrow_mut() = Some(e);
            let _ = slint::quit_event_loop();
        }
    });
    let m = manager.clone();
    let timer = slint::Timer::default();
    let guard = recovery.clone();
    let error = startup_error.clone();
    let mut session_started = false;
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(10),
        move || {
            let mut m = m.borrow_mut();
            for event in rx.try_iter().take(128) {
                m.event(event);
            }
            let Manager {
                shell,
                config,
                monitors,
                ..
            } = &mut *m;
            let pending = shell.pending;
            let ready = shell.arrange(config, monitors);
            if ready && pending && !m.shell.visible {
                m.focus_visible();
            }
            if ready
                && !session_started
                && guard.borrow().is_some()
                && !m.shell.backgrounds.is_empty()
            {
                session_started = true;
                // Explorer is stopped only after native surfaces exist, not at
                // watchdog startup. A failure also propagates to the exit code.
                let result = if replace {
                    session::explorer(false)
                } else {
                    Ok(())
                };
                match result {
                    Ok(()) => tracing::info!("Winarchy ready"),
                    Err(e) => {
                        tracing::error!(%e,"session initialization failed");
                        *error.borrow_mut() = Some(e);
                        let _ = slint::quit_event_loop();
                    }
                }
            }
        },
    );
    let m = manager.clone();
    let status = slint::Timer::default();
    status.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            let m = m.borrow();
            m.shell.refresh(&m.model, &m.config);
        },
    );
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    if let Some(e) = startup_error.borrow_mut().take() {
        return Err(e);
    }
    Ok(())
}
