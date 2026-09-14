use crate::config::Config;
use windows::Win32::{
    Foundation::FILETIME,
    Media::Audio::{Endpoints::*, *},
    System::{
        Com::*,
        Power::*,
        SystemInformation::{GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};
fn ticks(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}
/// Overall CPU load in percent. Sampled at most twice a second from
/// kernel/user/idle deltas, so several callers per refresh share one reading.
pub fn cpu_percent() -> Option<u64> {
    struct Sample {
        at: std::time::Instant,
        busy: u64,
        idle: u64,
        percent: Option<u64>,
    }
    static LAST: std::sync::Mutex<Option<Sample>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    if let Some(s) = &*last
        && now.duration_since(s.at) < std::time::Duration::from_millis(500)
    {
        return s.percent;
    }
    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let busy = ticks(kernel) + ticks(user);
    let idle = ticks(idle);
    let percent = last.as_ref().and_then(|s| {
        let total = busy.saturating_sub(s.busy);
        (total > 0).then(|| (total - idle.saturating_sub(s.idle).min(total)) * 100 / total)
    });
    *last = Some(Sample {
        at: now,
        busy,
        idle,
        percent,
    });
    percent
}
fn cpu() -> Option<String> {
    cpu_percent().map(|p| format!("{p}%"))
}
/// Available and total physical memory in GB, and the load percentage.
pub fn memory_status() -> Option<(f64, f64, u32)> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    let gb = |b: u64| b as f64 / (1024.0 * 1024.0 * 1024.0);
    Some((
        gb(status.ullAvailPhys),
        gb(status.ullTotalPhys),
        status.dwMemoryLoad,
    ))
}
fn memory() -> Option<String> {
    memory_status().map(|(available, _, _)| format!("{available:.1} GB"))
}
/// Battery percentage and whether AC power is connected; None without a battery.
pub fn battery_status() -> Option<(u8, bool)> {
    let mut p = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut p) }.ok()?;
    (p.BatteryLifePercent <= 100).then_some((p.BatteryLifePercent, p.ACLineStatus == 1))
}
fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate(CLSCTX_ALL, None).ok()
    }
}
/// Master volume in percent and mute state of the default output device.
pub fn volume_state() -> Option<(u32, bool)> {
    let endpoint = endpoint()?;
    unsafe {
        Some((
            (endpoint.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32,
            endpoint.GetMute().ok()?.as_bool(),
        ))
    }
}
/// `set <percent>`, `up`, `down` (5% steps) or `toggle-mute`.
pub fn volume_apply(action: &str) -> Result<(), String> {
    let endpoint = endpoint().ok_or("no audio output device")?;
    let (level, muted) = volume_state().ok_or("no audio output device")?;
    let target = match action.split_once(' ') {
        Some(("set", n)) => n
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("invalid volume: {n}"))?,
        None if action == "up" => i64::from(level) + 5,
        None if action == "down" => i64::from(level) - 5,
        None if action == "toggle-mute" => {
            return unsafe { endpoint.SetMute(!muted, std::ptr::null()) }
                .map_err(|e| e.to_string());
        }
        _ => return Err(format!("unknown volume action: {action}")),
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
fn volume() -> Option<String> {
    volume_state().map(|(level, muted)| {
        if muted {
            "muted".into()
        } else {
            format!("vol {level}%")
        }
    })
}
/// Module name and rendered value for each module that has something to show.
pub fn items(c: &Config, title: &str, modules: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for module in modules {
        let value = match module.as_str() {
            "window-title" => Some(title.to_owned()),
            "volume" => volume(),
            "clock" => unsafe {
                let t = GetLocalTime();
                Some(crate::clock::format(
                    &c.bar.clock_format,
                    crate::clock::Moment {
                        weekday: t.wDayOfWeek as u8,
                        day: t.wDay as u8,
                        month: t.wMonth as u8,
                        hour: t.wHour as u8,
                        minute: t.wMinute as u8,
                        second: t.wSecond as u8,
                    },
                ))
            },
            "battery" => battery_status().map(|(p, _)| format!("{p}%")),
            "cpu" => cpu(),
            "memory" => memory(),
            _ => None,
        };
        if let Some(value) = value {
            out.push((module.clone(), value));
        }
    }
    out
}
/// Popup content for a built-in module, when it has more to say than its label.
pub fn details(c: &Config, kind: &str) -> Option<(String, Vec<String>)> {
    match kind {
        "clock" => unsafe {
            let t = GetLocalTime();
            let moment = crate::clock::Moment {
                weekday: t.wDayOfWeek as u8,
                day: t.wDay as u8,
                month: t.wMonth as u8,
                hour: t.wHour as u8,
                minute: t.wMinute as u8,
                second: t.wSecond as u8,
            };
            Some((
                "DATE".into(),
                vec![
                    crate::clock::format("%A %d %B", moment) + &format!(" {}", t.wYear),
                    crate::clock::format("%H:%M:%S", moment),
                ],
            ))
        },
        "battery" => unsafe {
            let mut p = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut p).is_err() || p.BatteryLifePercent > 100 {
                return None;
            }
            let mut lines = vec![format!("Level: {}%", p.BatteryLifePercent)];
            lines.push(match p.ACLineStatus {
                1 => "Power: plugged in".into(),
                0 => "Power: on battery".into(),
                _ => "Power: unknown".into(),
            });
            if p.BatteryLifeTime != u32::MAX {
                lines.push(format!(
                    "Remaining: {}h{:02}",
                    p.BatteryLifeTime / 3600,
                    p.BatteryLifeTime % 3600 / 60
                ));
            }
            Some(("BATTERY".into(), lines))
        },
        "cpu" => {
            let load = cpu().unwrap_or_else(|| "measuring".into());
            Some((
                "CPU".into(),
                vec![
                    format!("Load: {load}"),
                    format!(
                        "Logical processors: {}",
                        std::thread::available_parallelism().map_or(0, |n| n.get())
                    ),
                ],
            ))
        }
        "memory" => {
            let (available, total, load) = memory_status()?;
            Some((
                "MEMORY".into(),
                vec![
                    format!("Available: {available:.1} GB"),
                    format!("In use: {:.1} GB ({load}%)", total - available),
                    format!("Total: {total:.1} GB"),
                ],
            ))
        }
        _ => {
            let _ = c;
            None
        }
    }
}
