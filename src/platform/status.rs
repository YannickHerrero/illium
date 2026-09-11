use crate::config::Config;
use windows::Win32::{
    Media::Audio::{Endpoints::*, *},
    System::{Com::*, Power::*, SystemInformation::GetLocalTime},
};
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
pub fn text(c: &Config, title: &str, modules: &[String]) -> String {
    let mut out = Vec::new();
    for module in modules {
        let value = match module.as_str() {
            "window-title" => Some(title.to_owned()),
            "volume" => volume(),
            "clock" => unsafe {
                let t = GetLocalTime();
                Some(
                    c.bar
                        .clock_format
                        .replace("%H", &format!("{:02}", t.wHour))
                        .replace("%M", &format!("{:02}", t.wMinute))
                        .replace("%S", &format!("{:02}", t.wSecond)),
                )
            },
            "battery" => unsafe {
                let mut p = SYSTEM_POWER_STATUS::default();
                if GetSystemPowerStatus(&mut p).is_ok() && p.BatteryLifePercent <= 100 {
                    Some(format!("{}%", p.BatteryLifePercent))
                } else {
                    None
                }
            },
            _ => None,
        };
        if let Some(value) = value {
            out.push(value);
        }
    }
    out.join("  ")
}
