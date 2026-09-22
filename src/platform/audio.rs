//! Core Audio access for the bar and the `builtin:volume` provider: default
//! endpoints, their levels, the active devices, the input peak level and the
//! per-application sessions of the default output.
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::CloseHandle,
        Media::Audio::{Endpoints::*, *},
        System::{
            Com::{StructuredStorage::*, *},
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{Interface, PCWSTR, PWSTR},
};

/// Windows has no public API to change the default endpoint; every switcher
/// (SoundSwitch, nircmd, AudioSwitcher) uses this COM interface, stable since
/// Vista. Only `SetDefaultEndpoint` is called; the earlier slots keep the
/// vtable layout and are never invoked.
mod policy {
    #![allow(non_snake_case, dead_code)]
    use super::*;
    use windows::core::{HRESULT, IUnknown, IUnknown_Vtbl};
    #[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub(super) unsafe trait IPolicyConfig: IUnknown {
        unsafe fn GetMixFormat(
            &self,
            device: PCWSTR,
            format: *mut *mut core::ffi::c_void,
        ) -> HRESULT;
        unsafe fn GetDeviceFormat(
            &self,
            device: PCWSTR,
            default: i32,
            format: *mut *mut core::ffi::c_void,
        ) -> HRESULT;
        unsafe fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
        unsafe fn SetDeviceFormat(
            &self,
            device: PCWSTR,
            endpoint: *const core::ffi::c_void,
            mix: *const core::ffi::c_void,
        ) -> HRESULT;
        unsafe fn GetProcessingPeriod(
            &self,
            device: PCWSTR,
            default: i32,
            default_period: *mut i64,
            minimum_period: *mut i64,
        ) -> HRESULT;
        unsafe fn SetProcessingPeriod(&self, device: PCWSTR, period: *const i64) -> HRESULT;
        unsafe fn GetShareMode(&self, device: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
        unsafe fn SetShareMode(&self, device: PCWSTR, mode: *const core::ffi::c_void) -> HRESULT;
        unsafe fn GetPropertyValue(
            &self,
            device: PCWSTR,
            key: *const core::ffi::c_void,
            value: *mut PROPVARIANT,
        ) -> HRESULT;
        unsafe fn SetPropertyValue(
            &self,
            device: PCWSTR,
            key: *const core::ffi::c_void,
            value: *const PROPVARIANT,
        ) -> HRESULT;
        unsafe fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
        unsafe fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
    }

    const POLICY_CONFIG_CLIENT: windows::core::GUID =
        windows::core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
    /// Makes `id` the default output or input for every role, like the Windows sound settings.
    pub(super) fn set_default(id: &str) -> Result<(), String> {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let policy: IPolicyConfig = CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                .map_err(|e| e.to_string())?;
            for role in [eConsole, eMultimedia, eCommunications] {
                policy
                    .SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role)
                    .ok()
                    .map_err(|e| format!("cannot switch device: {e}"))?;
            }
        }
        Ok(())
    }
}

const MAX_SESSIONS: usize = 16;

#[derive(Debug, PartialEq)]
enum Level {
    Set(i64),
    Up,
    Down,
    ToggleMute,
}
#[derive(Debug, PartialEq)]
enum Action {
    Output(Level),
    Input(Level),
    DefaultOutput(String),
    DefaultInput(String),
    Session(i64, String),
}
/// `set <n>`, `up`, `down`, `toggle-mute`, `input-set <n>`, `input-toggle-mute`,
/// `output <id>`, `input <id>` and `session <n> <id>`. The number precedes the
/// identifier because session identifiers may contain spaces.
fn parse(action: &str) -> Result<Action, String> {
    let percent = |n: &str| {
        n.trim()
            .parse::<i64>()
            .map_err(|_| format!("invalid volume: {n}"))
    };
    let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
    let rest = rest.trim();
    Ok(match (verb, rest) {
        ("set", n) => Action::Output(Level::Set(percent(n)?)),
        ("up", "") => Action::Output(Level::Up),
        ("down", "") => Action::Output(Level::Down),
        ("toggle-mute", "") => Action::Output(Level::ToggleMute),
        ("input-set", n) => Action::Input(Level::Set(percent(n)?)),
        ("input-toggle-mute", "") => Action::Input(Level::ToggleMute),
        ("output", id) if !id.is_empty() => Action::DefaultOutput(id.to_owned()),
        ("input", id) if !id.is_empty() => Action::DefaultInput(id.to_owned()),
        ("session", rest) => {
            let (n, id) = rest
                .split_once(' ')
                .ok_or("session action needs a level and an id")?;
            Action::Session(percent(n)?, id.trim().to_owned())
        }
        _ => return Err(format!("unknown volume action: {action}")),
    })
}

fn enumerator() -> Option<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
}
fn default_device(flow: EDataFlow) -> Option<IMMDevice> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(flow, eConsole).ok() }
}
fn endpoint(flow: EDataFlow) -> Option<IAudioEndpointVolume> {
    unsafe { default_device(flow)?.Activate(CLSCTX_ALL, None).ok() }
}
fn level(endpoint: &IAudioEndpointVolume) -> Option<(u32, bool)> {
    unsafe {
        Some((
            (endpoint.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32,
            endpoint.GetMute().ok()?.as_bool(),
        ))
    }
}
fn with_com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| e.to_string())?; }
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
    let _apartment = Apartment;
    f()
}
static BAR_LEVEL: std::sync::Mutex<Option<(u32, bool)>> = std::sync::Mutex::new(None);
static BAR_READING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
thread_local! {
    static BAR_READ_AT: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}
/// Nonblocking stale-while-revalidate bar reading. COM never runs on the UI.
pub fn volume_state() -> Option<(u32, bool)> {
    use std::sync::atomic::Ordering;
    let due = BAR_READ_AT.with(|last| last.get().is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(500)));
    if due && !BAR_READING.swap(true, Ordering::AcqRel) {
        BAR_READ_AT.with(|last| last.set(Some(std::time::Instant::now())));
        std::thread::spawn(|| {
            let value = with_com(|| Ok(endpoint(eRender).and_then(|e| level(&e)))).ok().flatten();
            *BAR_LEVEL.lock().unwrap_or_else(|e| e.into_inner()) = value;
            BAR_READING.store(false, Ordering::Release);
        });
    }
    *BAR_LEVEL.lock().unwrap_or_else(|e| e.into_inner())
}
/// Worker-only provider entry, with a balanced COM apartment per invocation.
pub fn query(action: Option<&str>, full: bool) -> Result<String, String> {
    with_com(|| {
        if let Some(action) = action.filter(|a| !matches!(*a, "refresh" | "levels" | "")) {
            apply(action)?;
        }
        let cheap = action.is_some_and(|a| a == "levels" || a.starts_with("set ")
            || a.starts_with("input-set ") || matches!(a, "up" | "down" | "toggle-mute" | "input-toggle-mute"));
        Ok(if cheap && !full { levels()? } else { snapshot()? }.to_string())
    })
}
/// Fast path: no device lists, session enumeration or process-name lookups.
pub fn levels() -> Result<serde_json::Value, String> {
    let output = endpoint(eRender).ok_or("no audio output device")?;
    let state = level(&output).ok_or("no audio output device")?;
    *BAR_LEVEL.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
    let (volume, muted) = state;
    let (input_volume, input_muted) = endpoint(eCapture).and_then(|e| level(&e)).unwrap_or((0, true));
    Ok(serde_json::json!({
        "volume": volume, "muted": muted, "input_volume": input_volume,
        "input_muted": input_muted, "input_level": input_level().unwrap_or(0),
    }))
}
/// Frees and converts a COM-allocated wide string.
unsafe fn take_string(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}
fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe { device.GetId().ok().map(|p| take_string(p)) }
}
fn friendly_name(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let text = PropVariantToStringAlloc(&value)
            .ok()
            .map(|p| take_string(p));
        let _ = PropVariantClear(&mut value);
        text
    }
}
fn devices(flow: EDataFlow) -> Vec<serde_json::Value> {
    let Some(enumerator) = enumerator() else {
        return vec![];
    };
    let default = default_device(flow).and_then(|d| device_id(&d));
    let mut out = Vec::new();
    unsafe {
        let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
            return out;
        };
        for i in 0..collection.GetCount().unwrap_or(0) {
            let Ok(device) = collection.Item(i) else {
                continue;
            };
            let Some(id) = device_id(&device) else {
                continue;
            };
            let name = friendly_name(&device).unwrap_or_else(|| id.clone());
            out.push(serde_json::json!({
                "id": id,
                "name": name,
                "default": default.as_deref() == Some(id.as_str()),
            }));
        }
    }
    out
}
/// Peak level of the default input in percent, sampled at call time.
fn input_level() -> Option<u32> {
    unsafe {
        let meter: IAudioMeterInformation =
            default_device(eCapture)?.Activate(CLSCTX_ALL, None).ok()?;
        Some(
            (meter.GetPeakValue().ok()? * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32,
        )
    }
}
fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 2048];
        let mut n = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n)
                .is_ok();
        let _ = CloseHandle(handle);
        let path = ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))?;
        let stem = std::path::Path::new(&path)
            .file_stem()?
            .to_string_lossy()
            .into_owned();
        (!stem.is_empty()).then_some(stem)
    }
}
/// Sessions of the default output that have not expired, with the volume
/// control of each; the callback receives the control so actions and the
/// listing share the enumeration.
fn each_session(mut f: impl FnMut(&IAudioSessionControl2, &ISimpleAudioVolume) -> bool) {
    unsafe {
        let Some(device) = default_device(eRender) else {
            return;
        };
        let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
            return;
        };
        let Ok(sessions) = manager.GetSessionEnumerator() else {
            return;
        };
        for i in 0..sessions.GetCount().unwrap_or(0) {
            let Ok(control) = sessions.GetSession(i) else {
                continue;
            };
            if control.GetState().ok() == Some(AudioSessionStateExpired) {
                continue;
            }
            let (Ok(control), Ok(volume)) = (
                control.cast::<IAudioSessionControl2>(),
                control.cast::<ISimpleAudioVolume>(),
            ) else {
                continue;
            };
            if !f(&control, &volume) {
                return;
            }
        }
    }
}
fn sessions() -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    each_session(|control, volume| unsafe {
        let Ok(id) = control
            .GetSessionInstanceIdentifier()
            .map(|p| take_string(p))
        else {
            return true;
        };
        let system = control.IsSystemSoundsSession() == windows::Win32::Foundation::S_OK;
        let display = control
            .GetDisplayName()
            .map(|p| take_string(p))
            .unwrap_or_default();
        let name = if system {
            "System sounds".to_owned()
        } else if !display.is_empty() && !display.starts_with('@') {
            display
        } else {
            control
                .GetProcessId()
                .ok()
                .filter(|pid| *pid != 0)
                .and_then(process_name)
                .unwrap_or_else(|| "Unknown".to_owned())
        };
        out.push(serde_json::json!({
            "id": id,
            "name": name,
            "volume": (volume.GetMasterVolume().unwrap_or(0.0) * 100.0).round() as u32,
            "muted": volume.GetMute().map(|m| m.as_bool()).unwrap_or(false),
        }));
        out.len() < MAX_SESSIONS
    });
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}
/// Everything the volume view shows, in one object.
pub fn snapshot() -> Result<serde_json::Value, String> {
    let output = endpoint(eRender).ok_or("no audio output device")?;
    let (volume, muted) = level(&output).ok_or("no audio output device")?;
    let (input_volume, input_muted) = endpoint(eCapture)
        .and_then(|e| level(&e))
        .unwrap_or((0, true));
    let outputs = devices(eRender);
    let output_name = outputs
        .iter()
        .find(|d| d["default"] == true)
        .and_then(|d| d["name"].as_str())
        .unwrap_or_default()
        .to_owned();
    Ok(serde_json::json!({
        "volume": volume,
        "muted": muted,
        "output_name": output_name,
        "outputs": outputs,
        "input_volume": input_volume,
        "input_muted": input_muted,
        "input_level": input_level().unwrap_or(0),
        "inputs": devices(eCapture),
        "sessions": sessions(),
    }))
}
fn adjust(endpoint: &IAudioEndpointVolume, change: Level, what: &str) -> Result<(), String> {
    let (level, muted) = level(endpoint).ok_or_else(|| format!("no audio {what} device"))?;
    let target = match change {
        Level::Set(n) => n,
        Level::Up => i64::from(level) + 5,
        Level::Down => i64::from(level) - 5,
        Level::ToggleMute => {
            return unsafe { endpoint.SetMute(!muted, std::ptr::null()) }
                .map_err(|e| e.to_string());
        }
    };
    let target = target.clamp(0, 100) as f32 / 100.0;
    unsafe {
        endpoint
            .SetMasterVolumeLevelScalar(target, std::ptr::null())
            .map_err(|e| e.to_string())?;
        if muted && target > 0.0 {
            let _ = endpoint.SetMute(false, std::ptr::null());
        }
    }
    Ok(())
}
fn set_session(target: i64, id: &str) -> Result<(), String> {
    let mut result = Err(format!("audio session not found: {id}"));
    each_session(|control, volume| unsafe {
        if control
            .GetSessionInstanceIdentifier()
            .map(|p| take_string(p))
            .as_deref()
            != Ok(id)
        {
            return true;
        }
        let level = target.clamp(0, 100) as f32 / 100.0;
        result = volume
            .SetMasterVolume(level, std::ptr::null())
            .map_err(|e| e.to_string());
        if level > 0.0 && volume.GetMute().map(|m| m.as_bool()) == Ok(true) {
            let _ = volume.SetMute(false, std::ptr::null());
        }
        false
    });
    result
}
/// Runs one view action; see [`parse`] for the grammar.
pub fn apply(action: &str) -> Result<(), String> {
    match parse(action)? {
        Action::Output(change) => adjust(
            &endpoint(eRender).ok_or("no audio output device")?,
            change,
            "output",
        ),
        Action::Input(change) => adjust(
            &endpoint(eCapture).ok_or("no audio input device")?,
            change,
            "input",
        ),
        Action::DefaultOutput(id) | Action::DefaultInput(id) => policy::set_default(&id),
        Action::Session(level, id) => set_session(level, &id),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions() {
        assert_eq!(parse("set 40").unwrap(), Action::Output(Level::Set(40)));
        assert_eq!(parse("up").unwrap(), Action::Output(Level::Up));
        assert_eq!(
            parse("toggle-mute").unwrap(),
            Action::Output(Level::ToggleMute)
        );
        assert_eq!(parse("input-set 0").unwrap(), Action::Input(Level::Set(0)));
        assert_eq!(
            parse("input-toggle-mute").unwrap(),
            Action::Input(Level::ToggleMute)
        );
        assert_eq!(
            parse("output {0.0.0.00000000}.{abc}").unwrap(),
            Action::DefaultOutput("{0.0.0.00000000}.{abc}".into())
        );
        assert_eq!(
            parse("session 55 {0.0.0.00000000}.{abc}|\\Device\\Harddisk\\Program Files\\app.exe%b{guid}").unwrap(),
            Action::Session(55, "{0.0.0.00000000}.{abc}|\\Device\\Harddisk\\Program Files\\app.exe%b{guid}".into())
        );
        for bad in ["", "set", "set x", "output", "session 5", "up 3", "mute"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
