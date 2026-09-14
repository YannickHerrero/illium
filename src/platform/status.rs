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
/// Overall CPU load since the previous call, from kernel/user/idle time deltas.
fn cpu() -> Option<String> {
    static LAST: std::sync::Mutex<Option<(u64, u64)>> = std::sync::Mutex::new(None);
    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let busy_total = ticks(kernel) + ticks(user);
    let idle = ticks(idle);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let previous = last.replace((busy_total, idle))?;
    let total = busy_total.saturating_sub(previous.0);
    if total == 0 {
        return None;
    }
    let idle = idle.saturating_sub(previous.1).min(total);
    Some(format!("{}%", (total - idle) * 100 / total))
}
fn memory() -> Option<String> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(format!(
        "{:.1} GB",
        status.ullAvailPhys as f64 / (1024.0 * 1024.0 * 1024.0)
    ))
}
fn volume() -> Option<String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        if endpoint.GetMute().ok()?.as_bool() {
            Some("muted".into())
        } else {
            Some(format!(
                "vol {}%",
                (endpoint.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32
            ))
        }
    }
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
            "battery" => unsafe {
                let mut p = SYSTEM_POWER_STATUS::default();
                if GetSystemPowerStatus(&mut p).is_ok() && p.BatteryLifePercent <= 100 {
                    Some(format!("{}%", p.BatteryLifePercent))
                } else {
                    None
                }
            },
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
            let mut status = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
            let gb = |b: u64| b as f64 / (1024.0 * 1024.0 * 1024.0);
            Some((
                "MEMORY".into(),
                vec![
                    format!("Available: {:.1} GB", gb(status.ullAvailPhys)),
                    format!(
                        "In use: {:.1} GB ({}%)",
                        gb(status.ullTotalPhys - status.ullAvailPhys),
                        status.dwMemoryLoad
                    ),
                    format!("Total: {:.1} GB", gb(status.ullTotalPhys)),
                ],
            ))
        }
        _ => {
            let _ = c;
            None
        }
    }
}
