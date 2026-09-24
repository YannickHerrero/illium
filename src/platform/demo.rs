//! One-shot demo startup. Identify our windows by owned process handles, never titles.
use super::native;
use std::{
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows::{Win32::UI::WindowsAndMessaging::GetPropW, core::w};

pub struct Pending {
    pub workspace: u8,
    pub monitor: usize,
    children: Vec<Child>, // build, fetch, browser, regardless of window arrival order
    deadline: Instant,
    next_poll: Instant,
    committed: bool,
}
impl Pending {
    pub fn start(workspace: u8, monitor: usize) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut pending = Self {
            workspace,
            monitor,
            children: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            next_poll: Instant::now(),
            committed: false,
        };
        for (name, args) in [
            ("winarchy-terminal.exe", vec!["--demo", "build"]),
            ("winarchy-terminal.exe", vec!["--demo", "fetch"]),
            ("winarchy-browser.exe", vec!["--demo"]),
        ] {
            let child = Command::new(executable.with_file_name(name))
                .args(args)
                .creation_flags(0x08000000) // CREATE_NO_WINDOW: no console/personal prompt.
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Cannot start demo {name}: {e}"))?;
            pending.children.push(child);
        }
        Ok(pending)
    }
    pub fn role(&self, id: isize) -> Option<usize> {
        let (pid, _) = native::process(id)?;
        self.children.iter().position(|child| child.id() == pid)
    }
    pub fn poll(&mut self) -> Result<Option<[isize; 3]>, String> {
        let now = Instant::now();
        if now < self.next_poll {
            return Ok(None);
        }
        self.next_poll = now + Duration::from_millis(100);
        for child in &mut self.children {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("A demo application closed before the scene was ready".into());
            }
        }
        let mut windows = [0; 3];
        for id in native::enumerate() {
            if let Some(role) = self.role(id)
                && native::visible(id)
                && !unsafe { GetPropW(native::hwnd(id), w!("WinarchyDemoReady")) }.is_invalid()
            {
                windows[role] = id;
            }
        }
        if windows.iter().all(|id| *id != 0) {
            return Ok(Some(windows));
        }
        if now >= self.deadline {
            return Err(
                "Demo startup timed out (30s); update all Winarchy executables together".into(),
            );
        }
        Ok(None)
    }
    pub fn commit(&mut self) {
        self.committed = true;
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut children = std::mem::take(&mut self.children);
        // Keep handles alive, allow graceful profile cleanup, then reap only OUR
        // processes. Never kill by executable name or reuse a stale numeric PID.
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            loop {
                children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
                if children.is_empty() {
                    return;
                }
                for id in native::enumerate() {
                    if let Some((pid, _)) = native::process(id)
                        && children.iter().any(|child| child.id() == pid)
                    {
                        native::close(id);
                    }
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            for mut child in children {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
    }
}

impl super::Manager {
    pub(super) fn start_demo(&mut self) -> Result<String, String> {
        if self.demo.is_some() {
            return Err("A demo is already starting".into());
        }
        // Reconcile pending show events before deciding the workspace is empty.
        for id in native::enumerate() {
            self.add(id);
        }
        if self.model.clients.iter().any(|c| self.model.shown(c)) {
            return Err("Demo requires an empty workspace (including minimized windows)".into());
        }
        self.demo_error = None;
        self.shell.dismiss();
        self.shell.close_popup();
        self.shell.picker.close();
        self.applets.close();
        self.finish_editor(super::shell::keybindings::Outcome::Close);
        match Pending::start(
            self.model.active,
            self.model.monitors[usize::from(self.model.active - 1)],
        ) {
            Ok(pending) => self.demo = Some(pending),
            Err(error) => {
                self.demo_error = Some(error.clone());
                return Err(error);
            }
        }
        Ok("Demo starting; status exposes demo_pending and demo_error".into())
    }
    pub(super) fn poll_demo(&mut self) {
        let Some(pending) = self.demo.as_mut() else {
            return;
        };
        let result = if self.model.active != pending.workspace {
            Err("Demo cancelled: workspace changed during startup".into())
        } else if self
            .model
            .clients
            .iter()
            .any(|c| c.on(self.model.space, pending.workspace) && pending.role(c.id).is_none())
        {
            Err("Demo cancelled: another window entered the workspace".into())
        } else {
            pending.poll()
        };
        let windows = match result {
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "demo startup failed");
                self.demo_error = Some(error);
                self.demo = None; // closes only owned children, off the UI thread
                return;
            }
            Ok(Some(windows)) => windows,
        };
        for id in windows {
            self.add(id);
        }
        if windows
            .iter()
            .any(|id| !self.model.clients.iter().any(|c| c.id == *id))
        {
            self.demo_error = Some("Could not enroll all demo windows".into());
            self.demo = None;
            return;
        }
        // Stable sorting preserves all unrelated workspaces. The default three-
        // leaf Fibonacci tree is exactly left / top-right / bottom-right.
        self.model.clients.sort_by_key(|c| {
            windows
                .iter()
                .position(|id| *id == c.id)
                .map(|i| i + 1)
                .unwrap_or(0)
        });
        for client in &mut self.model.clients {
            if windows.contains(&client.id) {
                client.workspace = self.model.active;
                client.floating = false;
                client.fullscreen = false;
            }
        }
        self.model.splits[usize::from(self.model.active - 1)] = Default::default();
        self.pending_browser_focus = None;
        self.model.focused = Some(windows[0]);
        self.layout();
        native::focus(windows[0], true);
        self.demo.as_mut().unwrap().commit();
        self.demo = None;
        tracing::info!("demo scene ready");
    }
}
