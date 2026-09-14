//! Applet runtime: schedules data providers, holds their JSON, compiles the
//! Slint views on demand and shows them as anchored popups.
use super::{Event, EventSender, native, shell};
use crate::{
    applets::{self, Applet},
    config::Config,
    layout::Rect,
};
use slint::ComponentHandle;
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
use std::time::{Duration, Instant};
pub struct Entry {
    pub applet: Applet,
    pub icon: Option<slint::Image>,
    pub data: serde_json::Value,
    pub error: Option<String>,
    running: bool,
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
}
impl Runtime {
    pub fn new(tx: EventSender) -> Self {
        Self {
            entries: vec![],
            open: None,
            pending: None,
            tx,
        }
    }
    /// Loads the applets the bar references; data of applets that stay is kept.
    pub fn load(&mut self, c: &Config) {
        self.close();
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
                due: Instant::now(),
                definition: None,
                instance: None,
                applet,
            });
        }
    }
    pub fn is_applet(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.applet.name == name)
    }
    /// Bar label and icon for an applet module.
    pub fn item(&self, name: &str) -> Option<(String, Option<slint::Image>)> {
        let e = self.entries.iter().find(|e| e.applet.name == name)?;
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
        e.due = Instant::now() + e.interval;
        if let Some(provider) = &e.applet.manifest.provider {
            let result = builtin(provider).map_err(|e| e.to_string());
            let name = e.applet.name.clone();
            self.apply(&name, result);
            return;
        }
        e.running = true;
        let name = e.applet.name.clone();
        let command = applets::command(&e.applet);
        let env = applets::environment(&e.applet);
        let dir = e.applet.dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = run(&command, &env, &dir, action.as_deref());
            let _ = tx.send(Event::AppletData(name, result));
        });
    }
    /// Stores a provider result and pushes it to the open view.
    pub fn apply(&mut self, name: &str, result: Result<String, String>) {
        let Some(e) = self.entries.iter_mut().find(|e| e.applet.name == name) else {
            return;
        };
        e.running = false;
        match result.and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|err| format!("invalid JSON: {err}"))
        }) {
            Ok(data) => {
                e.data = data;
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
        for (prop, value) in [
            ("bg", &c.theme.background),
            ("surface", &c.theme.surface),
            ("overlay", &c.theme.overlay),
            ("fg", &c.theme.text),
            ("muted", &c.theme.subtext),
            ("accent", &c.theme.accent),
        ] {
            let _ = instance.set_property(
                prop,
                Value::Brush(slint::Brush::SolidColor(shell::color(value))),
            );
        }
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
fn builtin(provider: &str) -> Result<String, String> {
    match provider {
        "builtin:clock" => Ok(clock().to_string()),
        "builtin:system" => Ok(system().to_string()),
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
