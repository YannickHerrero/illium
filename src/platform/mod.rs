mod applet;
mod apps;
mod audio;
mod browser;
mod dictate;
mod dpi;
use winarchy_ipc::identity;
mod input;
mod instance;
pub mod ipc;
mod native;
mod security;
mod session;
#[cfg(test)]
mod session_tests;
mod shell;
mod status;
mod terminal;
use crate::{
    command::Command,
    config::Config,
    layout::{Rect, fibonacci, neighbor},
    model::{Client, Model},
    state::{Placement, State},
};
pub use security::require_standard_user;
pub use session::watchdog;
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{
    Foundation::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};
pub type EventSender = crate::queue::Sender<Event>;
pub enum Event {
    Command(Command, Option<crate::request::ReplyTo>),
    Window(u32, isize),
    Search(String),
    Launch(i32),
    Dismiss,
    Reload,
    Wallpapers,
    /// Preview assets changed independently of configuration and the active wallpaper.
    ThemePreviews,
    /// Input is tagged with the picker opening generation to reject stale events.
    Picker(u64, shell::theme_picker::Input),
    Display,
    Mouse(isize),
    /// Bar module clicked: kind, horizontal center in logical bar pixels, monitor index.
    Module(String, i32, usize),
    /// Left button pressed on the root window `isize`, anywhere on the desktop.
    Click(isize),
    /// Empty bar area clicked.
    BarBackground,
    /// An applet provider finished: applet name and its stdout or error.
    AppletData(String, u64, Result<String, String>),
    AppletTraffic {
        name: String,
        generation: u64,
        interface: String,
        result: Result<crate::traffic::Sample, String>,
    },
    /// An applet view asked for an action to be run by its provider.
    AppletAction(String, Option<String>),
    /// Escape pressed while a bar popup was open.
    Escape,
    /// Backspace on an empty launcher query: leave a submenu.
    Back,
    /// Keybindings editor input, tagged with its opening generation.
    Keybindings(u64, shell::keybindings::Input),
    /// A key seen by the hook while the editor records a chord: virtual key,
    /// modifier mask and whether it went down.
    Capture(u32, u8, bool),
    /// The `dictate` key went down (`true`) or up (`false`).
    Dictate(bool),
}
struct Manager {
    config: Config,
    config_files: Vec<(std::path::PathBuf, Vec<u8>)>,
    model: Model,
    monitors: Vec<Rect>,
    shell: shell::Shell,
    borders: std::collections::HashMap<isize, native::Border>,
    applets: applet::Runtime,
    /// Popup closed by a press on the bar: the module click that follows the
    /// release must not reopen it.
    just_closed: Option<(String, std::time::Instant)>,
    /// New browser to focus after its final tile geometry has been applied.
    pending_browser_focus: Option<isize>,
    /// Placement may have changed since the last save.
    dirty: bool,
}
impl Manager {
    fn prune(&mut self) -> bool {
        let previous = self.model.clients.len();
        // A client that is invisible without Winarchy having hidden it has left
        // the desktop on its own; keeping it would make it a dead focus target.
        for c in &self.model.clients {
            if !c.hidden && !native::minimized(c.id) && !native::visible(c.id) {
                session::untag(c.id);
            }
        }
        self.model
            .clients
            .retain(|c| session::owns(c.id, c.generation));
        if self.model.focused.is_some_and(|id| {
            !self
                .model
                .clients
                .iter()
                .any(|c| c.id == id && c.workspace == self.model.active && !native::minimized(c.id))
        }) {
            self.model.focused = None;
        }
        previous != self.model.clients.len()
    }
    fn add(&mut self, id: isize) -> bool {
        let added = self.enroll(id, None);
        if added
            && native::metadata(id).is_some_and(|(_, class, _)| class == "WinarchyBrowser")
            && self
                .model
                .clients
                .iter()
                .any(|c| c.id == id && c.workspace == self.model.active)
        {
            self.model.focused = Some(id);
            self.pending_browser_focus = Some(id);
        }
        added
    }
    /// A remembered placement wins over the active workspace, the rules and
    /// the floating heuristics: the user had already arranged that window.
    fn enroll(&mut self, id: isize, saved: Option<&Placement>) -> bool {
        if self.prune() {
            self.layout();
        }
        if let Some(c) = self.model.clients.iter_mut().find(|c| c.id == id) {
            if c.workspace != self.model.active {
                c.hidden = true;
                native::show(id, false);
            }
            return false;
        }
        if self.model.clients.len() >= 512 {
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
        let (mut fullscreen, mut restore) = (false, native::rect(id));
        if let Some(p) = saved {
            workspace = p.workspace;
            floating = p.floating;
            fullscreen = p.fullscreen;
            restore = p.restore;
        }
        let generation = match session::tag(id) {
            Ok(g) => g,
            Err(e) => {
                tracing::debug!(id,%e,"window cannot be tagged safely");
                return false;
            }
        };
        if self.config.wm.square_corners {
            native::corners(id, true);
        }
        self.model.clients.push(Client {
            id,
            generation,
            workspace,
            floating,
            fullscreen,
            hidden: false,
            restore,
        });
        tracing::info!(id, workspace, "window added");
        if apps::wants_focus(&exe) || terminal::wants_focus(&exe) {
            self.model.focused = Some(id);
            native::focus(id, false);
        }
        true
    }
    fn snapshot(&self) -> State {
        State {
            active: self.model.active,
            recent: self.model.recent,
            monitors: self.model.monitors,
            clients: self
                .model
                .clients
                .iter()
                .filter_map(|c| {
                    let (pid, exe) = native::process(c.id)?;
                    Some(Placement {
                        id: c.id,
                        pid,
                        exe,
                        workspace: c.workspace,
                        floating: c.floating,
                        fullscreen: c.fullscreen,
                        restore: c.restore,
                    })
                })
                .collect(),
        }
    }
    fn full_area(&self) -> Rect {
        let index = self.model.monitors[(self.model.active - 1) as usize]
            .min(self.monitors.len().saturating_sub(1));
        self.monitors.get(index).copied().unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 1280,
            h: 720,
        })
    }
    fn area(&self) -> Rect {
        let mut r = self.full_area();
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
        self.dirty = true;
        self.prune();
        for c in &mut self.model.clients {
            let visible = c.workspace == self.model.active;
            if visible != native::visible(c.id) {
                native::show(c.id, visible);
            }
            c.hidden = !visible;
        }
        let area = self.area();
        let ids: Vec<_> = self
            .model
            .clients
            .iter()
            .filter(|c| {
                c.workspace == self.model.active
                    && !c.floating
                    && !c.fullscreen
                    && !native::minimized(c.id)
            })
            .map(|c| c.id)
            .collect();
        let rs = fibonacci(
            area,
            ids.len(),
            dpi::scale(area, self.config.wm.gap),
            dpi::scale(area, self.config.wm.outer_gap),
        );
        native::batch(
            &ids.into_iter()
                .zip(rs)
                .map(|(id, r)| (id, native::framed(id, r)))
                .collect::<Vec<_>>(),
        );
        for c in &self.model.clients {
            if c.workspace == self.model.active && c.fullscreen && !native::minimized(c.id) {
                native::position(c.id, native::framed(c.id, area), Some(HWND_TOP));
            }
        }
        if let Some(id) = self.pending_browser_focus.take()
            && self
                .model
                .clients
                .iter()
                .any(|c| c.id == id && c.workspace == self.model.active && !c.hidden)
        {
            // Warp only once, after tiling, not during subsequent page resizes.
            native::focus(id, true);
        }
        self.shell.refresh(&self.model, &self.config, &self.applets);
        self.borders();
    }
    fn borders(&mut self) {
        let width = self.config.wm.border_width.clamp(0, 32);
        if width == 0 {
            self.borders.clear();
            return;
        }
        self.borders
            .retain(|id, _| self.model.clients.iter().any(|c| c.id == *id));
        for c in &self.model.clients {
            let shown = c.workspace == self.model.active
                && !c.fullscreen
                && !native::minimized(c.id)
                && native::visible(c.id);
            if !shown {
                if let Some(b) = self.borders.get(&c.id) {
                    b.hide();
                }
                continue;
            }
            let color = if Some(c.id) == self.model.focused {
                &self.config.theme.accent
            } else {
                &self.config.theme.overlay
            };
            native::dwm_border(c.id, Some(color));
            let frame = native::frame(c.id);
            // The DWM edge pixel is the innermost pixel of the visible border.
            let ring = dpi::scale(frame, width) - 1;
            if ring <= 0 {
                if let Some(b) = self.borders.get(&c.id) {
                    b.hide();
                }
                continue;
            }
            if let Some(b) = match self.borders.entry(c.id) {
                std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut()),
                std::collections::hash_map::Entry::Vacant(e) => {
                    native::Border::new().map(|b| e.insert(b))
                }
            } {
                b.place(c.id, frame, ring, color);
            }
        }
    }
    fn focus_visible(&mut self) {
        if self.shell.picker.opened {
            return;
        }
        if self.prune() {
            self.layout();
        }
        let id = self
            .model
            .focused
            .filter(|id| {
                self.model.clients.iter().any(|c| {
                    c.id == *id && c.workspace == self.model.active && !native::minimized(c.id)
                })
            })
            .or_else(|| {
                self.model
                    .clients
                    .iter()
                    .find(|c| c.workspace == self.model.active && !native::minimized(c.id))
                    .map(|c| c.id)
            });
        self.model.focused = id;
        native::focus(id.unwrap_or_else(session::sink), id.is_some());
    }
    fn reload(&mut self) -> Result<(), String> {
        self.reload_config(true)
    }
    fn reload_config(&mut self, force: bool) -> Result<(), String> {
        let started = std::time::Instant::now();
        let files = crate::files::snapshot(&self.config.home)?;
        if !force && files == self.config_files {
            return Ok(()); // e.g. watcher notification after an IPC theme change
        }
        let config = Config::load(&self.config.home)?;
        if config.global.theme != self.config.global.theme {
            winarchy_theme::opacity::clear(&config.home)?;
        }
        if !force && crate::files::same_subsystems(&config.home, &self.config_files, &files) {
            if config.global.theme != self.config.global.theme || config.theme != self.config.theme
            {
                self.shell.apply_theme(&config);
                self.applets.apply_theme(&config);
                if config.theme.mode != self.config.theme.mode
                    && let Some(mode) = &config.theme.mode
                {
                    native::color_mode(mode == "light");
                }
                self.config = config;
                self.borders();
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis(),
                    "theme updated in place"
                );
            }
            self.config_files = files;
            return Ok(());
        }
        let bindings = input::parse(&config.keys)?;
        self.shell.configure(&config, &self.monitors)?;
        dictate::prewarm(&bindings);
        input::update(bindings);
        let terminal_changed =
            config.apps.apps.get("terminal") != self.config.apps.apps.get("terminal");
        let browser_changed =
            config.apps.apps.get("browser") != self.config.apps.apps.get("browser");
        self.config = config;
        if browser_changed {
            let target = self.config.apps.apps.get("browser");
            if target.is_some_and(|s| browser::bundled(s)) {
                browser::prewarm(target);
            } else {
                browser::stop_idle();
            }
        }
        if terminal_changed {
            terminal::prewarm(self.config.apps.apps.get("terminal"));
        }
        self.applets.load(&self.config);
        self.shell.editor.refresh(&self.config);
        let shell_ms = started.elapsed().as_millis();
        if let Some(mode) = &self.config.theme.mode {
            native::color_mode(mode == "light");
        }
        for c in &self.model.clients {
            native::corners(c.id, self.config.wm.square_corners);
            if self.config.wm.border_width <= 0 {
                native::dwm_border(c.id, None);
            }
        }
        self.layout();
        self.config_files = files;
        tracing::info!(
            shell_ms,
            total_ms = started.elapsed().as_millis(),
            "configuration reloaded"
        );
        Ok(())
    }
    fn execute(&mut self, c: Command) -> Result<String, String> {
        if self.prune() {
            self.layout();
        }
        match &c {
            Command::Spawn(_) | Command::LaunchTarget { .. } => {
                tracing::debug!("application launch command")
            }
            _ => tracing::debug!(?c, "command"),
        }
        let foreground = unsafe { GetForegroundWindow().0 as isize };
        if self.model.clients.iter().any(|w| {
            w.id == foreground && w.workspace == self.model.active && !native::minimized(w.id)
        }) {
            self.model.focused = Some(foreground);
        }
        match c {
            Command::Status => return Ok(serde_json::json!({
                "workspace": self.model.active, "recent": self.model.recent,
                "focused": self.model.focused, "theme": self.config.global.theme,
                "wallpaper": self.shell.wallpaper,
                "wallpaper_pending": self.shell.pending_wallpaper(),
                "wallpaper_error": self.shell.wallpaper_error,
                "gap": self.config.wm.gap, "launcher": self.shell.visible,
                "theme_picker": self.shell.picker.opened && self.shell.picker.wallpaper_theme.is_none(),
                "wallpaper_picker": self.shell.picker.opened && self.shell.picker.wallpaper_theme.is_some(),
                "wallpaper_picker_theme": self.shell.picker.wallpaper_theme,
                "wallpaper_picker_selected": self.shell.picker.wallpaper_theme.as_ref().and(self.shell.picker.selected_id()),
                "wallpaper_picker_loading": self.shell.picker.wallpaper_theme.is_some() && self.shell.picker.loading(),
                "wallpaper_picker_filter": self.shell.picker.wallpaper_theme.as_ref().map(|_| self.shell.picker.filter()),
                "wallpaper_picker_error": self.shell.picker.wallpaper_theme.as_ref().and(self.shell.picker.error.as_ref()),
                "theme_picker_selected": self.shell.picker.wallpaper_theme.is_none().then(|| self.shell.picker.selected_id()).flatten(),
                "theme_picker_filter": if self.shell.picker.wallpaper_theme.is_none() { self.shell.picker.filter() } else { "" },
                "theme_picker_loading": self.shell.picker.wallpaper_theme.is_none() && self.shell.picker.loading(),
                "theme_picker_error": self.shell.picker.wallpaper_theme.is_none().then_some(self.shell.picker.error.as_ref()).flatten(),
                "monitors": self.monitors, "bar_count": self.shell.bars.len(), "bar_transparent": self.shell.bar_transparent,
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
                                && !native::minimized(w.id)
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
                            native::focus(other, true);
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
                                w.restore = native::framed(w.id, r);
                                native::position(w.id, w.restore, None);
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
                if shortcut { native::shortcut(&target)?; }
                else if terminal::bundled(&target) { terminal::open(); }
                else if browser::bundled(&target) { browser::open(); }
                else { native::spawn(&target)?; }
            }
            Command::Launcher => {
                self.shell.picker.close();
                self.shell.toggle(&self.config, self.area())?;
            }
            Command::Meta => {
                self.shell.picker.close();
                self.shell.toggle_meta(&self.config, self.area())?;
            }
            Command::Keybindings => {
                if self.shell.editor.opened {
                    self.finish_editor(shell::keybindings::Outcome::Close);
                } else {
                    let restore = self.restore_target(foreground);
                    self.shell.dismiss();
                    self.shell.close_popup();
                    self.applets.close();
                    self.finish_picker(crate::theme_picker::Outcome::Cancel);
                    let monitor = self.full_area();
                    self.shell.editor.open(&self.config, monitor, restore);
                }
            }
            Command::App(name) => apps::open(name),
            Command::Dictate => dictate::toggle(),
            Command::Reload => self.reload()?,
            Command::ThemePicker | Command::WallpaperPicker => {
                if !self.shell.picker.opened {
                    let restore = self.restore_target(foreground);
                    self.finish_editor(shell::keybindings::Outcome::Close);
                    self.shell.dismiss();
                    self.shell.close_popup();
                    self.applets.close();
                    if matches!(c, Command::WallpaperPicker) {
                        let selected = self.shell.pending_wallpaper().or(self.shell.wallpaper.as_deref()).map(str::to_owned);
                        self.shell.picker.open_wallpapers(&self.config, self.full_area(), restore, selected.as_deref())?;
                    } else {
                        self.shell.picker.open(&self.config, self.full_area(), restore)?;
                    }
                }
            }
            Command::WallpaperNext => self.shell.next_wallpaper()?,
            Command::Wallpaper(name) => self.shell.set_wallpaper(name)?,
            Command::BackgroundOpacity(increase) => {
                let mut theme = self.config.theme.clone();
                winarchy_theme::opacity::apply(
                    &self.config.home, &self.config.global.theme, &mut theme,
                );
                let opacity = winarchy_theme::opacity::step(theme.background_opacity, increase);
                winarchy_theme::opacity::set(
                    &self.config.home, &self.config.global.theme, opacity,
                )?;
                return Ok(format!("background opacity: {:.0}%", opacity * 100.0));
            }
            Command::Theme(name) => {
                let path = self.config.home.join("winarchy.toml");
                let old = crate::files::read_config(&path)?;
                std::fs::write(&path, format!("theme = {name:?}\n")).map_err(|e| e.to_string())?;
                if let Err(e) = self.reload_config(false) {
                    let _ = std::fs::write(path, old);
                    return Err(e);
                }
            }
            Command::Explorer(start) => session::explorer(start)?,
            Command::Quit => {
                apps::stop_resident();
                terminal::stop_idle();
                browser::stop_idle();
                dictate::stop_resident();
                slint::quit_event_loop().map_err(|e| e.to_string())?;
            }
        }
        Ok("ok".into())
    }
    /// Window to refocus when a full-screen surface closes: the foreground
    /// window unless it belongs to Winarchy itself.
    fn restore_target(&self, foreground: isize) -> Option<isize> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(native::hwnd(foreground), Some(&mut pid));
        }
        if pid != std::process::id() && foreground != 0 {
            Some(foreground)
        } else {
            self.model.focused
        }
    }
    fn restore_focus(&mut self, restore: Option<isize>) {
        if let Some(id) = restore.filter(|id| native::visible(*id) && !native::minimized(*id)) {
            native::focus(id, false);
        } else {
            self.focus_visible();
        }
    }
    fn finish_picker(&mut self, outcome: crate::theme_picker::Outcome) {
        use crate::theme_picker::Outcome;
        if outcome == Outcome::None {
            return;
        }
        let restore = self.shell.picker.close();
        if let Outcome::Apply(name) = outcome
            && let Some(command) = self
                .shell
                .picker
                .selection_command(name, &self.config.global.theme)
            && let Err(error) = self.execute(command)
        {
            tracing::warn!(%error, "selected carousel item could not be applied");
            self.shell.picker.error = Some(error);
        }
        self.restore_focus(restore);
    }
    fn finish_editor(&mut self, outcome: shell::keybindings::Outcome) {
        use shell::keybindings::Outcome;
        match outcome {
            Outcome::None => {}
            Outcome::Close => {
                if self.shell.editor.opened {
                    let restore = self.shell.editor.close();
                    self.restore_focus(restore);
                }
            }
            Outcome::Apply(apply) => {
                if let Err(e) = self.apply_keybinding(apply) {
                    tracing::warn!(%e, "keybinding not applied");
                    self.shell.editor.fail(e);
                }
            }
        }
    }
    /// Rewrites the user's file and reloads; a rejected reload restores the
    /// previous text so the running configuration and the file stay in step.
    fn apply_keybinding(&mut self, apply: shell::keybindings::Apply) -> Result<(), String> {
        let path = self.config.home.join("keybindings.toml");
        let old = crate::files::read_config(&path)?;
        let text = std::str::from_utf8(&old).map_err(|e| e.to_string())?;
        let remove: Vec<&str> = apply.remove.iter().map(String::as_str).collect();
        let new = crate::keybindings::rewrite(text, &remove, &apply.chord, &apply.command)?;
        std::fs::write(&path, new).map_err(|e| e.to_string())?;
        if let Err(e) = self.reload_config(false) {
            let _ = std::fs::write(path, old);
            return Err(e);
        }
        Ok(())
    }
    fn event(&mut self, event: Event) {
        self.dispatch(event);
        input::POPUP_OPEN.store(
            self.shell.popup_open.is_some() || self.applets.open.is_some(),
            std::sync::atomic::Ordering::Relaxed,
        );
        input::CAPTURE.store(
            self.shell.editor.capturing(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    fn dispatch(&mut self, event: Event) {
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
                        // A delayed destroy event must not remove a new window
                        // that has reused the same numeric HWND.
                        self.prune();
                        self.layout();
                        if self.model.focused.is_none() {
                            self.focus_visible();
                        }
                    }
                }
                EVENT_OBJECT_HIDE => {
                    if self.model.clients.iter().any(|c| c.id == id) && self.prune() {
                        self.layout();
                    }
                }
                EVENT_SYSTEM_FOREGROUND => {
                    if let Some(c) = self.model.clients.iter().find(|c| c.id == id)
                        && c.workspace == self.model.active
                        && !native::minimized(c.id)
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
                            self.dirty = true;
                        }
                    }
                    self.shell.refresh(&self.model, &self.config, &self.applets);
                    self.borders();
                }
                EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW | EVENT_OBJECT_UNCLOAKED => {
                    if self.add(id) {
                        self.layout();
                    }
                }
                EVENT_SYSTEM_MINIMIZEEND => {
                    self.add(id);
                    self.layout();
                }
                EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MOVESIZEEND => self.layout(),
                _ => {}
            },
            Event::Picker(epoch, input) => {
                let outcome = self.shell.picker.input(epoch, input);
                self.finish_picker(outcome);
            }
            Event::ThemePreviews => self.shell.picker.rescan(),
            Event::Keybindings(epoch, input) => {
                let outcome = self.shell.editor.input(epoch, input);
                self.finish_editor(outcome);
            }
            Event::Capture(vk, modifiers, down) => {
                let outcome = self.shell.editor.capture(vk, modifiers, down);
                self.finish_editor(outcome);
            }
            Event::Search(q) => self.shell.search(&q, self.config.launcher.max_results),
            Event::Launch(n) if self.shell.meta => {
                let max = self.config.launcher.max_results;
                if let Some(command) = self.shell.meta_activate(n.max(0) as usize, max) {
                    self.shell.dismiss();
                    if let Err(e) = self.execute(command) {
                        tracing::error!(%e,"session action failed");
                    }
                }
            }
            Event::Back => self.shell.meta_back(self.config.launcher.max_results),
            Event::Launch(n) => {
                if let Some(app) = self.shell.results.get(n.max(0) as usize).cloned() {
                    self.shell.dismiss();
                    let command = match app.app {
                        Some(name) => Command::App(name),
                        None => Command::LaunchTarget {
                            target: app.target,
                            shortcut: app.shortcut,
                        },
                    };
                    let result = self.execute(command);
                    if let Err(e) = result {
                        tracing::error!(%e,"launch failed");
                    }
                }
            }
            Event::Dismiss => {
                self.shell.dismiss();
                self.focus_visible();
            }
            Event::Module(kind, x, monitor) => {
                if self.shell.picker.opened {
                    self.finish_picker(crate::theme_picker::Outcome::Cancel);
                }
                self.finish_editor(shell::keybindings::Outcome::Close);
                let target = self.applets.attached(&kind).unwrap_or_else(|| kind.clone());
                if self
                    .just_closed
                    .take()
                    .is_some_and(|(k, at)| k == target && at.elapsed().as_millis() < 500)
                {
                    return;
                }
                let r = self
                    .monitors
                    .get(monitor)
                    .copied()
                    .unwrap_or_else(|| self.area());
                if self.applets.is_applet(&target) {
                    self.shell.close_popup();
                    if let Err(e) = self.applets.toggle(&self.config, r, &target, x) {
                        tracing::warn!(applet = %target, %e, "applet view unavailable");
                        let lines = e.lines().take(8).map(str::to_owned).collect();
                        let title = format!("APPLET {target}");
                        self.shell
                            .open_popup(&self.config, r, kind, x, title, lines);
                    }
                } else if self.shell.popup_open.as_deref() == Some(kind.as_str()) {
                    self.shell.close_popup();
                } else if let Some((title, lines)) = status::details(&self.config, &kind) {
                    self.applets.close();
                    self.shell
                        .open_popup(&self.config, r, kind, x, title, lines);
                }
            }
            Event::Click(id) => {
                // Any press outside the open popup closes it, the bar included.
                if !self.applets.owns(id) && self.shell.popup_hwnd() != id {
                    let open = self
                        .applets
                        .open
                        .clone()
                        .or_else(|| self.shell.popup_open.clone());
                    self.shell.close_popup();
                    self.applets.close();
                    self.just_closed = open.map(|k| (k, std::time::Instant::now()));
                }
            }
            Event::BarBackground => {
                // The press already closed an open popup through Event::Click;
                // that click only dismisses, it does not toggle.
                if self
                    .just_closed
                    .take()
                    .is_some_and(|(_, at)| at.elapsed().as_millis() < 500)
                {
                    return;
                }
                self.shell.toggle_bar_background();
            }
            Event::Escape => {
                self.shell.close_popup();
                self.applets.escape();
            }
            Event::Dictate(down) => dictate::hold(down),
            Event::AppletData(name, generation, result) => {
                self.applets.apply(&name, generation, result);
                self.shell.refresh(&self.model, &self.config, &self.applets);
            }
            Event::AppletTraffic {
                name,
                generation,
                interface,
                result,
            } => {
                self.applets
                    .apply_traffic(&name, generation, &interface, result);
            }
            Event::AppletAction(name, action) => self.applets.action(&name, action),
            Event::Mouse(id) => {
                if self.config.wm.focus_follows_mouse
                    && !self.shell.interactive()
                    && self
                        .model
                        .clients
                        .iter()
                        .any(|c| c.id == id && c.workspace == self.model.active)
                {
                    self.model.focused = Some(id);
                    native::focus(id, false);
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
                // WM_SETTINGCHANGE can change DPI without changing physical bounds.
                let monitor = self.full_area();
                self.shell.picker.display_changed(monitor);
                self.shell.editor.display_changed(monitor);
            }
            Event::Wallpapers => {
                self.shell.refresh_wallpaper();
                // Invalidate preloaded wallpaper views even when closed or
                // while the last-used surface was the theme selector.
                self.shell.picker.rescan();
            }
            Event::Reload => {
                if let Err(e) = self.reload_config(false) {
                    tracing::warn!(%e,"keeping previous configuration");
                }
            }
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        if let Err(error) = winarchy_theme::opacity::clear(&self.config.home) {
            tracing::warn!(%error, "could not clear temporary opacity");
        }
        for c in &self.model.clients {
            if !session::owns(c.id, c.generation) {
                continue;
            }
            native::show(c.id, true);
            native::corners(c.id, false);
            native::dwm_border(c.id, None);
            session::untag(c.id);
            if c.fullscreen {
                native::position(c.id, c.restore, None);
            }
        }
    }
}
fn watch(home: std::path::PathBuf, tx: EventSender) {
    std::thread::spawn(move || unsafe {
        use windows::{
            Win32::{Storage::FileSystem::*, System::Threading::*},
            core::PCWSTR,
        };
        let path = native::wide(&home.to_string_lossy());
        let Ok(h) = FindFirstChangeNotificationW(
            PCWSTR(path.as_ptr()),
            true,
            FILE_NOTIFY_CHANGE_LAST_WRITE
                | FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_DIR_NAME,
        ) else {
            return;
        };
        let wallpaper_snapshot = || {
            let images = winarchy_theme::Theme::selected(&home).and_then(|theme| {
                winarchy_theme::pack::fingerprint(&home, &theme).map(|images| (theme, images))
            });
            (
                images,
                crate::files::read_config(&home.join("wallpapers.json")),
            )
        };
        let mut previous = crate::files::snapshot(&home);
        let mut previous_wallpapers = wallpaper_snapshot();
        let mut previous_previews = winarchy_theme::preview::catalog(&home);
        loop {
            if WaitForSingleObject(h, INFINITE) != WAIT_OBJECT_0 {
                break;
            }
            if FindNextChangeNotification(h).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            let next = crate::files::snapshot(&home);
            let next_wallpapers = wallpaper_snapshot();
            // The picker covers inactive themes too. This bounded scan lives on
            // the notification thread and never decodes pixels or reloads applets.
            let next_previews = winarchy_theme::preview::catalog(&home);
            if next != previous {
                previous = next;
                let _ = tx.send(Event::Reload);
            }
            // Config and image edits can be coalesced into one notification.
            // An already-applied IPC theme change must not hide the image edit.
            if next_wallpapers != previous_wallpapers {
                let _ = tx.send(Event::Wallpapers);
            }
            previous_wallpapers = next_wallpapers;
            if next_previews != previous_previews {
                let _ = tx.send(Event::ThemePreviews);
            }
            previous_previews = next_previews;
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
    winarchy_theme::opacity::clear(&home)?;
    let state_path = home.join("state.json");
    let config_files = crate::files::snapshot(&home)?;
    let config = Config::load(&home)?;
    let bindings = input::parse(&config.keys)?;
    let (tx, rx) = crate::queue::channel(1024);
    let maintenance = tx.clone();
    ipc::start(tx.clone())?;
    let manager = Manager {
        config,
        config_files,
        model: Model::new(),
        monitors: native::monitors(),
        shell: shell::Shell::new(tx.clone())?,
        borders: std::collections::HashMap::new(),
        pending_browser_focus: None,
        applets: applet::Runtime::new(tx.clone()),
        just_closed: None,
        dirty: false,
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
            // Subscribe before enumeration so no show/create/restore event can
            // disappear between the initial snapshot and hook registration.
            dictate::prewarm(&bindings);
            input::start(tx.clone(), bindings)?;
            let foreground = unsafe { GetForegroundWindow().0 as isize };
            m.shell.configure(&config, &monitors)?;
            m.applets.load(&config);
            if let Some(mode) = &config.theme.mode {
                native::color_mode(mode == "light");
            }
            let mut state = State::load(&home.join("state.json")).unwrap_or_default();
            state.retain_alive(native::process);
            if !state.clients.is_empty() {
                m.model.active = state.active;
                m.model.recent = state.recent;
                m.model.monitors = state.monitors;
                for index in &mut m.model.monitors {
                    *index = (*index).min(monitors.len().saturating_sub(1));
                }
            }
            for id in native::enumerate() {
                m.enroll(id, state.placement(id));
            }
            m.model.clients.sort_by_key(|c| state.order(c.id));
            tracing::info!(
                count = m.model.clients.len(),
                restored = state.clients.len(),
                "existing application windows enrolled"
            );
            m.model.focused = m
                .model
                .clients
                .iter()
                .find(|c| c.id == foreground)
                .map(|c| c.id);
            m.layout();
            m.focus_visible();
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
    let mut preload_at = std::time::Instant::now() + std::time::Duration::from_secs(2);
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
            shell.poll_wallpaper();
            let outcome = shell.picker.poll();
            let pending = shell.pending;
            let ready = shell.arrange(config, monitors);
            m.finish_picker(outcome);
            m.applets.arrange();
            if ready && std::time::Instant::now() >= preload_at {
                preload_at = std::time::Instant::now() + std::time::Duration::from_millis(500);
                if !m.shell.interactive() && m.shell.pending_wallpaper().is_none() {
                    let monitor = m.full_area();
                    let Manager { shell, config, .. } = &mut *m;
                    let selected = shell.wallpaper.clone();
                    shell.picker.preload(config, monitor, selected.as_deref());
                }
            }
            if ready && pending && !m.shell.interactive() {
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
                    Ok(()) => {
                        tracing::info!("Winarchy ready");
                        apps::start_resident();
                        terminal::prewarm(m.config.apps.apps.get("terminal"));
                        browser::prewarm(m.config.apps.apps.get("browser"));
                    }
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
    let mut saved = String::new();
    status.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            let mut m = m.borrow_mut();
            if m.prune() {
                m.layout();
            }
            // Layout runs in bursts; the tick debounces them into one write.
            if m.dirty {
                m.dirty = false;
                let json = m.snapshot().to_json();
                if json != saved {
                    match State::save(&json, &state_path) {
                        Ok(()) => saved = json,
                        Err(e) => tracing::warn!(%e, "placement state not saved"),
                    }
                }
            }
            if maintenance.take_overflow() {
                tracing::warn!("event queue overflow; reconciling windows and configuration");
                m.event(Event::Display);
                let active = m.model.active;
                m.model.clients.retain(|c| {
                    let keep = c.workspace != active
                        || unsafe { IsWindowVisible(native::hwnd(c.id)).as_bool() };
                    if !keep {
                        session::untag(c.id);
                    }
                    keep
                });
                for id in native::enumerate() {
                    m.add(id);
                }
                if let Err(e) = m.reload() {
                    tracing::warn!(%e,"overflow reload rejected");
                    m.layout();
                }
            }
            m.applets.tick();
            m.shell.refresh(&m.model, &m.config, &m.applets);
            m.borders();
        },
    );
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    if let Some(e) = startup_error.borrow_mut().take() {
        return Err(e);
    }
    Ok(())
}
