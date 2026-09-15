//! Applet runtime: schedules data providers, holds their JSON, compiles the
//! Slint views on demand and shows them as anchored popups.
mod traffic;
use super::{Event, EventSender, native, shell};
use crate::{
    applets::{self, Applet},
    config::Config,
    layout::Rect,
};
use slint::ComponentHandle;
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
#[cfg(test)]
mod scheduling_tests {
    use super::*;
    fn runtime() -> Runtime {
        let (tx, _) = crate::queue::channel(8);
        let mut runtime = Runtime::new(tx);
        runtime.generation = 2;
        runtime.entries.push(Entry {
            applet: Applet {
                name: "wifi".into(),
                dir: std::path::PathBuf::new(),
                manifest: toml::from_str("").unwrap(),
            },
            icon: None,
            data: serde_json::json!({"ssid":"current"}),
            error: None,
            running: true,
            traffic: None,
            pending_actions: VecDeque::new(),
            due: Instant::now(),
            interval: Duration::from_secs(30),
            definition: None,
            instance: None,
        });
        runtime
    }
    #[test]
    fn busy_actions_are_bounded_and_ordered() {
        let mut runtime = runtime();
        runtime.action("wifi", None);
        assert!(runtime.entries[0].pending_actions.is_empty());
        for i in 0..12 {
            runtime.action("wifi", Some(i.to_string()));
        }
        assert_eq!(runtime.entries[0].pending_actions.len(), 8);
        assert_eq!(runtime.entries[0].pending_actions.front().unwrap(), "0");
        assert_eq!(runtime.entries[0].pending_actions.back().unwrap(), "7");
    }
    #[test]
    fn stale_provider_results_do_not_clear_a_new_worker() {
        let mut runtime = runtime();
        runtime.apply("wifi", 1, Ok(r#"{"ssid":"old"}"#.into()));
        assert_eq!(runtime.entries[0].data["ssid"], "current");
        assert!(runtime.entries[0].running);
        runtime.apply("wifi", 2, Ok(r#"{"ssid":"new"}"#.into()));
        assert_eq!(runtime.entries[0].data["ssid"], "new");
        assert!(!runtime.entries[0].running);
    }
}
fn set_colors(instance: &ComponentInstance, theme: &crate::config::Theme) {
    for (prop, value) in [
        ("bg", &theme.background),
        ("surface", &theme.surface),
        ("overlay", &theme.overlay),
        ("fg", &theme.text),
        ("muted", &theme.subtext),
        ("accent", &theme.accent),
    ] {
        let _ = instance.set_property(
            prop,
            Value::Brush(slint::Brush::SolidColor(shell::color(value))),
        );
    }
}
pub struct Entry {
    pub applet: Applet,
    pub icon: Option<slint::Image>,
    pub data: serde_json::Value,
    pub error: Option<String>,
    running: bool,
    traffic: Option<traffic::Monitor>,
    pending_actions: VecDeque<String>,
    due: Instant,
    interval: Duration,
    definition: Option<ComponentDefinition>,
    instance: Option<ComponentInstance>,
}
pub struct Runtime {
    pub entries: Vec<Entry>,
    /// Name of the applet whose view is shown.
    pub open: Option<String>,
    pending: Option<Rect>,
    tx: EventSender,
    generation: u64,
}
impl Runtime {
    pub fn new(tx: EventSender) -> Self {
        Self {
            entries: vec![],
            open: None,
            pending: None,
            tx,
            generation: 0,
        }
    }
    /// Loads the applets the bar references; data of applets that stay is kept.
    pub fn load(&mut self, c: &Config) {
        self.close();
        self.generation = self.generation.wrapping_add(1);
        let previous = std::mem::take(&mut self.entries);
        for loaded in applets::referenced(&c.home, &[&c.bar.left, &c.bar.center, &c.bar.right]) {
            let applet = match loaded {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(%e, "applet skipped");
                    continue;
                }
            };
            let old = previous.iter().find(|e| e.applet.name == applet.name);
            let icon = slint::Image::load_from_path(&applet.dir.join(&applet.manifest.icon))
                .map_err(|e| tracing::warn!(applet = %applet.name, %e, "applet icon not loaded"))
                .ok();
            self.entries.push(Entry {
                interval: applets::interval(&applet.manifest.interval)
                    .unwrap_or(Duration::from_secs(60)),
                icon,
                data: old.map_or(serde_json::Value::Null, |o| o.data.clone()),
                error: None,
                running: false,
                traffic: applet.manifest.wifi_traffic.then(traffic::Monitor::default),
                pending_actions: VecDeque::new(),
                due: Instant::now(),
                definition: None,
                instance: None,
                applet,
            });
        }
    }
    /// Recolor existing views without resetting providers, data, intervals or compiled definitions.
    pub fn apply_theme(&self, c: &Config) {
        for entry in &self.entries {
            if let Some(instance) = &entry.instance {
                set_colors(instance, &c.theme);
            }
        }
    }
    pub fn is_applet(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.applet.name == name)
    }
    /// The applet that opens when the built-in `module` is clicked.
    pub fn attached(&self, module: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.applet.manifest.attach.as_deref() == Some(module))
            .map(|e| e.applet.name.clone())
    }
    /// Bar label and icon for an applet module; attached applets have none.
    pub fn item(&self, name: &str) -> Option<(String, Option<slint::Image>)> {
        let e = self
            .entries
            .iter()
            .find(|e| e.applet.name == name && e.applet.manifest.attach.is_none())?;
        let label = match (&e.error, &e.applet.manifest.label) {
            (Some(_), _) => "!".into(),
            (None, Some(t)) => applets::label(t, &e.data),
            (None, None) => String::new(),
        };
        Some((label, e.icon.clone()))
    }
    /// Starts the providers that are due.
    pub fn tick(&mut self) {
        let now = Instant::now();
        for i in 0..self.entries.len() {
            let e = &mut self.entries[i];
            if let Some(traffic) = &mut e.traffic {
                traffic.poll(
                    &e.applet.name,
                    self.generation,
                    self.open.as_deref() == Some(&e.applet.name),
                    &self.tx,
                );
            }
            if !self.entries[i].running && self.entries[i].due <= now {
                self.refresh(i, None);
            }
        }
    }
    pub fn action(&mut self, name: &str, action: Option<String>) {
        if let Some(i) = self.entries.iter().position(|e| e.applet.name == name) {
            self.refresh(i, action);
        }
    }
    fn refresh(&mut self, index: usize, action: Option<String>) {
        let e = &mut self.entries[index];
        if e.running {
            if let Some(action) = action {
                if e.pending_actions.len() < 8 {
                    e.pending_actions.push_back(action);
                } else {
                    tracing::warn!(applet = %e.applet.name, "applet action queue full");
                }
            }
            return;
        }
        e.due = Instant::now() + e.interval;
        if let Some(provider) = &e.applet.manifest.provider {
            let result = builtin(provider, action.as_deref());
            let name = e.applet.name.clone();
            self.apply(&name, self.generation, result);
            return;
        }
        e.running = true;
        if let Some(instance) = &e.instance {
            let _ = instance.set_property("busy", Value::Bool(true));
        }
        let generation = self.generation;
        let name = e.applet.name.clone();
        let command = applets::command(&e.applet);
        let env = applets::environment(&e.applet);
        let dir = e.applet.dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = run(&command, &env, &dir, action.as_deref());
            let _ = tx.send(Event::AppletData(name, generation, result));
        });
    }
    /// Stores a provider result and pushes it to the open view.
    pub fn apply(&mut self, name: &str, generation: u64, result: Result<String, String>) {
        if generation != self.generation {
            return;
        }
        let Some(index) = self.entries.iter().position(|e| e.applet.name == name) else {
            return;
        };
        let e = &mut self.entries[index];
        e.running = false;
        if let Some(instance) = &e.instance {
            let _ = instance.set_property("busy", Value::Bool(false));
        }
        match result.and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|err| format!("invalid JSON: {err}"))
        }) {
            Ok(data) => {
                e.data = data;
                if let Some(traffic) = &mut e.traffic {
                    let interface = if e.data["connected"].as_bool() == Some(true) {
                        e.data["interface_guid"].as_str().unwrap_or_default()
                    } else {
                        ""
                    };
                    traffic.select(interface);
                    traffic.merge(&mut e.data);
                }
                e.error = None;
                if let Some(instance) = &e.instance
                    && let Some(def) = &e.definition
                    && let Err(err) = set_data(instance, def, &e.data)
                {
                    e.error = Some(err);
                }
            }
            Err(err) => {
                tracing::warn!(applet = name, %err, "applet provider failed");
                e.error = Some(err);
            }
        }
        if let Some(instance) = &e.instance {
            let _ = instance.set_property(
                "provider-error",
                Value::String(e.error.clone().unwrap_or_default().into()),
            );
        }
        if let Some(action) = e.pending_actions.pop_front() {
            self.refresh(index, Some(action));
        }
    }
    pub fn apply_traffic(
        &mut self,
        name: &str,
        generation: u64,
        interface: &str,
        result: Result<crate::traffic::Sample, String>,
    ) {
        if generation != self.generation {
            return;
        }
        let Some(e) = self.entries.iter_mut().find(|e| e.applet.name == name) else {
            return;
        };
        let Some(traffic) = &mut e.traffic else {
            return;
        };
        traffic.finish(interface, result);
        traffic.merge(&mut e.data);
        if let (Some(instance), Some(def)) = (&e.instance, &e.definition)
            && let Err(error) = set_data(instance, def, &e.data)
        {
            tracing::warn!(%error, "traffic view update failed");
        }
    }
    /// Shows or hides the applet view under the bar module centered at logical `x`.
    pub fn toggle(&mut self, c: &Config, monitor: Rect, name: &str, x: i32) -> Result<(), String> {
        if self.open.as_deref() == Some(name) {
            self.close();
            return Ok(());
        }
        self.close();
        let Some(e) = self.entries.iter_mut().find(|e| e.applet.name == name) else {
            return Err(format!("unknown applet {name}"));
        };
        if let Some(err) = &e.error {
            return Err(err.clone());
        }
        if e.definition.is_none() {
            e.definition = Some(compile(&e.applet)?);
        }
        if let Some(traffic) = &mut e.traffic {
            traffic.due = Instant::now();
            if let Some(data) = e.data.as_object_mut() {
                data.insert("receiving".into(), serde_json::json!("—"));
                data.insert("sending".into(), serde_json::json!("—"));
            }
        }
        let def = e.definition.as_ref().expect("compiled above");
        let instance = match &e.instance {
            Some(i) => i.clone_strong(),
            None => {
                let instance = def.create().map_err(|err| err.to_string())?;
                let tx = self.tx.clone();
                let applet = name.to_owned();
                let _ = instance.set_callback("action", move |args| {
                    let arg = match args.first() {
                        Some(Value::String(s)) => Some(s.to_string()),
                        _ => None,
                    };
                    let _ = tx.send(Event::AppletAction(applet.clone(), arg));
                    Value::Void
                });
                e.instance = Some(instance.clone_strong());
                instance
            }
        };
        set_colors(&instance, &c.theme);
        let _ = instance.set_property("busy", Value::Bool(e.running));
        if e.data != serde_json::Value::Null {
            set_data(&instance, def, &e.data)?;
        }
        let size = &e.applet.manifest.popup;
        let (w, h) = (
            super::dpi::scale(monitor, size.width).min(monitor.w),
            super::dpi::scale(monitor, size.height).min(monitor.h),
        );
        let _ = instance.set_property("popup-width", Value::Number(size.width as f64));
        let _ = instance.set_property("popup-height", Value::Number(size.height as f64));
        let bar = super::dpi::scale(monitor, c.bar.height);
        let gap = super::dpi::scale(monitor, 6);
        let center = monitor.x + super::dpi::scale(monitor, x);
        self.pending = Some(Rect {
            x: (center - w / 2).clamp(monitor.x, monitor.x + monitor.w - w),
            y: if c.bar.position == "top" {
                monitor.y + bar + gap
            } else {
                monitor.y + monitor.h - bar - gap - h
            },
            w,
            h,
        });
        instance.show().map_err(|err| err.to_string())?;
        self.open = Some(name.to_owned());
        Ok(())
    }
    pub fn close(&mut self) {
        if let Some(name) = self.open.take()
            && let Some(e) = self.entries.iter().find(|e| e.applet.name == name)
            && let Some(instance) = &e.instance
        {
            let _ = instance.hide();
        }
        self.pending = None;
    }
    /// Positions the view once its native window exists.
    pub fn arrange(&mut self) {
        let Some(r) = self.pending else { return };
        let Some(e) = self
            .open
            .as_ref()
            .and_then(|name| self.entries.iter().find(|e| e.applet.name == *name))
        else {
            self.pending = None;
            return;
        };
        let Some(instance) = &e.instance else { return };
        let window = instance.window();
        if shell::id(window) == 0 {
            return;
        }
        shell::tool(window, !e.applet.manifest.focusable);
        native::position(
            shell::id(window),
            r,
            Some(windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST),
        );
        self.pending = None;
    }
    pub fn owns(&self, id: isize) -> bool {
        self.entries
            .iter()
            .filter_map(|e| e.instance.as_ref())
            .any(|i| shell::id(i.window()) == id)
    }
}
fn compile(applet: &Applet) -> Result<ComponentDefinition, String> {
    let mut compiler = slint_interpreter::Compiler::new();
    compiler.set_include_paths(vec![applet.dir.clone()]);
    let result = spin_on::spin_on(compiler.build_from_path(applet.dir.join("view.slint")));
    if result.has_errors() {
        let text = result
            .diagnostics()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("view.slint: {text}"));
    }
    result
        .component("View")
        .or_else(|| {
            result
                .component_names()
                .next()
                .and_then(|n| result.component(n))
        })
        .ok_or_else(|| "view.slint exports no component".to_owned())
}
fn set_data(
    instance: &ComponentInstance,
    def: &ComponentDefinition,
    data: &serde_json::Value,
) -> Result<(), String> {
    let Some((_, (ty, _))) = def.properties_and_callbacks().find(|(n, _)| n == "data") else {
        return Ok(());
    };
    let value = slint_interpreter::json::value_from_json(&ty, &prune(data, &ty))
        .map_err(|e| format!("data does not match the view's `data` property: {e}"))?;
    instance
        .set_property("data", value)
        .map_err(|e| format!("cannot set data: {e:?}"))
}
/// Drops JSON fields the view does not declare, so providers may print more
/// than a view consumes; the converter rejects unknown fields otherwise.
fn prune(data: &serde_json::Value, ty: &i_slint_compiler::langtype::Type) -> serde_json::Value {
    use i_slint_compiler::langtype::Type;
    match (data, ty) {
        (serde_json::Value::Object(fields), Type::Struct(declared)) => serde_json::Value::Object(
            fields
                .iter()
                .filter_map(|(k, v)| {
                    let key = k.replace('_', "-");
                    declared
                        .fields
                        .get(key.as_str())
                        .map(|t| (key, prune(v, t)))
                })
                .collect(),
        ),
        (serde_json::Value::Array(items), Type::Array(inner)) => {
            serde_json::Value::Array(items.iter().map(|v| prune(v, inner)).collect())
        }
        _ => data.clone(),
    }
}
/// Runs the provider hidden, bounded in time and output, in the applet folder.
fn run(
    command: &[String],
    env: &[(String, String)],
    dir: &std::path::Path,
    action: Option<&str>,
) -> Result<String, String> {
    use std::{io::Read, os::windows::process::CommandExt};
    let (program, args) = command.split_first().ok_or("empty command")?;
    let mut child = std::process::Command::new(program)
        .args(args)
        .args(action)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("cannot start {program}: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout
            .take((applets::MAX_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut out);
        let mut err = Vec::new();
        let _ = stderr.take(4096).read_to_end(&mut err);
        (out, err)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > applets::TIMEOUT => {
                let _ = child.kill();
                return Err(format!(
                    "provider timed out after {}s",
                    applets::TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (out, err) = reader.join().map_err(|_| "reader failed")?;
    if out.len() > applets::MAX_OUTPUT_BYTES {
        return Err("provider output exceeds 64 KiB".into());
    }
    if !status.success() {
        let err = String::from_utf8_lossy(&err).trim().to_owned();
        return Err(if err.is_empty() {
            format!("provider exited with {status}")
        } else {
            err.lines().take(3).collect::<Vec<_>>().join(" ")
        });
    }
    String::from_utf8(out).map_err(|e| e.to_string())
}
/// Data for `builtin:clock` and `builtin:system`, without a child process.
/// Built-in providers answer on the UI thread; only `builtin:volume` acts on
/// an action, the others return their reading whatever the view asked.
fn builtin(provider: &str, action: Option<&str>) -> Result<String, String> {
    match provider {
        "builtin:clock" => Ok(clock().to_string()),
        "builtin:system" => Ok(system().to_string()),
        "builtin:volume" => {
            if let Some(action) = action {
                super::status::volume_apply(action)?;
            }
            let (volume, muted) = super::status::volume_state().ok_or("no audio output device")?;
            Ok(serde_json::json!({ "volume": volume, "muted": muted }).to_string())
        }
        other => Err(format!("unknown provider {other}")),
    }
}
fn clock() -> serde_json::Value {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let leap = t.wYear % 4 == 0 && (t.wYear % 100 != 0 || t.wYear % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ][usize::from(t.wMonth.clamp(1, 12)) - 1];
    // Monday-first column of the 1st: Windows counts Sunday as 0.
    let first = (i32::from(t.wDayOfWeek) - i32::from(t.wDay) + 1).rem_euclid(7);
    let first = (first + 6) % 7;
    let mut weeks = Vec::new();
    let mut week = Vec::new();
    for _ in 0..first {
        week.push(serde_json::json!({"day": 0, "current": false, "today": false}));
    }
    for day in 1..=days {
        week.push(serde_json::json!({"day": day, "current": true, "today": day == t.wDay}));
        if week.len() == 7 {
            weeks.push(serde_json::Value::Array(std::mem::take(&mut week)));
        }
    }
    if !week.is_empty() {
        while week.len() < 7 {
            week.push(serde_json::json!({"day": 0, "current": false, "today": false}));
        }
        weeks.push(serde_json::Value::Array(week));
    }
    let moment = crate::clock::Moment {
        weekday: t.wDayOfWeek as u8,
        day: t.wDay as u8,
        month: t.wMonth as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        second: t.wSecond as u8,
    };
    serde_json::json!({
        "year": t.wYear, "month": t.wMonth, "day": t.wDay, "weekday": t.wDayOfWeek,
        "hour": t.wHour, "minute": t.wMinute, "second": t.wSecond,
        "month_name": crate::clock::format("%B", moment),
        "weekday_name": crate::clock::format("%A", moment),
        "time": crate::clock::format("%H:%M", moment),
        "weeks": weeks,
    })
}
fn system() -> serde_json::Value {
    let (available, total, load) = super::status::memory_status().unwrap_or((0.0, 0.0, 0));
    let battery = super::status::battery_status();
    serde_json::json!({
        "cpu": super::status::cpu_percent().unwrap_or(0),
        "memory_available_gb": (available * 10.0).round() / 10.0,
        "memory_total_gb": (total * 10.0).round() / 10.0,
        "memory_load": load,
        "battery": battery.map_or(-1, |(p, _)| i32::from(p)),
        "plugged": battery.is_some_and(|(_, ac)| ac),
        "processors": std::thread::available_parallelism().map_or(0, |n| n.get()),
    })
}
