//! Battery panel logic behind the `builtin:battery` provider: view actions,
//! Windows power modes, formatting of the readings and the memory that lets
//! the battery saver and travel mode restore what they changed.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Battery saver threshold that keeps it on whenever the machine runs on battery.
pub const SAVER_ALWAYS: u32 = 100;
/// Windows' own default, used when the threshold to restore is unknown.
pub const SAVER_DEFAULT: u32 = 20;
pub const TRAVEL_BRIGHTNESS: u8 = 40;

/// The three positions of the Windows power mode setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Saver,
    Balanced,
    Performance,
}
const OVERLAY_SAVER: u128 = 0x961cc777_2547_4f9d_8174_7d86181b8a7a;
const OVERLAY_PERFORMANCE: u128 = 0xded574b5_45a0_4f42_8737_46345c09c238;
/// Windows 10 reports its "better performance" position with this identifier.
const OVERLAY_BALANCED_LEGACY: u128 = 0x3af9b8d9_7c97_431d_ad78_34a8bfea439f;
impl Mode {
    pub fn from_overlay(guid: u128) -> Option<Self> {
        match guid {
            OVERLAY_SAVER => Some(Self::Saver),
            0 | OVERLAY_BALANCED_LEGACY => Some(Self::Balanced),
            OVERLAY_PERFORMANCE => Some(Self::Performance),
            _ => None,
        }
    }
    pub fn overlay(self) -> u128 {
        match self {
            Self::Saver => OVERLAY_SAVER,
            Self::Balanced => 0,
            Self::Performance => OVERLAY_PERFORMANCE,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Saver => "saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Refresh,
    Mode(Mode),
    Brightness(u8),
    Saver(bool),
    Travel(bool),
}
/// `refresh`, `mode <saver|balanced|performance>`, `brightness <0-100>`,
/// `saver <on|off>` and `travel <on|off>`.
pub fn parse(action: &str) -> Result<Action, String> {
    let switch = |s: &str| match s {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("invalid battery action: {action}")),
    };
    let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
    Ok(match (verb, rest.trim()) {
        ("refresh" | "", "") => Action::Refresh,
        ("mode", "saver") => Action::Mode(Mode::Saver),
        ("mode", "balanced") => Action::Mode(Mode::Balanced),
        ("mode", "performance") => Action::Mode(Mode::Performance),
        ("brightness", n) => Action::Brightness(
            n.parse::<i64>()
                .map_err(|_| format!("invalid brightness: {n}"))?
                .clamp(0, 100) as u8,
        ),
        ("saver", s) => Action::Saver(switch(s)?),
        ("travel", s) => Action::Travel(switch(s)?),
        _ => return Err(format!("unknown battery action: {action}")),
    })
}

/// One reading of the batteries, summed when a machine has several.
/// Capacities are in mWh; a value the hardware does not report is `None`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Reading {
    pub percent: u8,
    pub plugged: bool,
    pub charging: bool,
    pub design_mwh: Option<u32>,
    pub full_mwh: Option<u32>,
    pub remaining_mwh: Option<u32>,
    /// Positive while charging, negative while discharging.
    pub rate_mw: Option<i32>,
    pub cycles: Option<u32>,
    /// Windows' own estimate of the time left on battery.
    pub lifetime_s: Option<u32>,
}
impl Reading {
    pub fn health(&self) -> Option<u32> {
        let (full, design) = (self.full_mwh?, self.design_mwh?);
        (design > 0)
            .then(|| ((u64::from(full) * 100 + u64::from(design) / 2) / u64::from(design)) as u32)
    }
    /// "Not charging" while plugged in below full is how a firmware charge
    /// limit or a too-weak charger shows up, whatever the vendor.
    pub fn state(&self) -> &'static str {
        match (self.plugged, self.charging) {
            (_, true) => "Charging",
            (true, false) if self.percent >= 99 => "Full",
            (true, false) => "Not charging",
            (false, _) => "On battery",
        }
    }
    pub fn time(&self) -> String {
        match (self.charging, self.plugged, self.rate_mw) {
            (true, _, Some(rate)) if rate > 0 => {
                let (Some(full), Some(remaining)) = (self.full_mwh, self.remaining_mwh) else {
                    return String::new();
                };
                let minutes = u64::from(full.saturating_sub(remaining)) * 60 / rate as u64;
                format!("{} to full", duration(minutes))
            }
            (false, false, rate) => {
                let minutes = self.lifetime_s.map(|s| u64::from(s) / 60).or_else(|| {
                    let rate = rate.filter(|r| *r < 0)?.unsigned_abs();
                    Some(u64::from(self.remaining_mwh?) * 60 / u64::from(rate))
                });
                minutes.map_or_else(String::new, |m| format!("{} left", duration(m)))
            }
            _ => String::new(),
        }
    }
    pub fn power(&self) -> String {
        match self.rate_mw {
            Some(rate) if rate != 0 => format!("{:.1} W", f64::from(rate.unsigned_abs()) / 1000.0),
            _ => String::new(),
        }
    }
}
fn duration(minutes: u64) -> String {
    format!("{}h{:02}", minutes / 60, minutes % 60)
}
pub fn watt_hours(mwh: Option<u32>) -> String {
    mwh.map_or_else(String::new, |v| format!("{:.1} Wh", f64::from(v) / 1000.0))
}
/// Label and value of every figure this battery reports, in display order.
pub fn stats(r: &Reading) -> Vec<(&'static str, String)> {
    [
        ("Design capacity", watt_hours(r.design_mwh)),
        ("Full charge", watt_hours(r.full_mwh)),
        (
            "Health",
            r.health().map_or_else(String::new, |h| format!("{h}%")),
        ),
        (
            "Cycles",
            r.cycles.map_or_else(String::new, |c| c.to_string()),
        ),
        ("Power", r.power()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .collect()
}
/// What travel mode changes on this machine; empty when it can change nothing.
pub fn travel_summary(mode: bool, brightness: bool, saver: bool) -> String {
    let parts: Vec<String> = [
        mode.then(|| "Power saver mode".to_owned()),
        brightness.then(|| format!("{TRAVEL_BRIGHTNESS}% brightness at most")),
        saver.then(|| "battery saver".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();
    match parts.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// What a battery saver or travel toggle must write; `None` leaves a setting alone.
#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    pub mode: Option<Mode>,
    pub brightness: Option<u8>,
    pub saver_threshold: Option<u32>,
}
/// Settings as read before a toggle; `None` where the machine lacks the control.
#[derive(Debug, Default, Clone, Copy)]
pub struct Current {
    pub mode: Option<Mode>,
    pub brightness: Option<u8>,
    pub saver_threshold: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Travel {
    pub mode: Option<Mode>,
    pub brightness: Option<u8>,
    pub saver_forced: bool,
}
/// Values to restore, kept across daemon restarts.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub saver_threshold: Option<u32>,
    pub travel: Option<Travel>,
}
impl Memory {
    /// A missing or unreadable file means nothing to restore.
    pub fn load(path: &Path) -> Self {
        crate::files::read_bounded(path, 4096)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if *self == Self::default() {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
    /// The threshold to write so the saver is forced on or back to its setting.
    pub fn saver(&mut self, on: bool, current: Option<u32>) -> Option<u32> {
        let current = current?;
        match (on, current == SAVER_ALWAYS) {
            (true, false) => {
                self.saver_threshold = Some(current);
                Some(SAVER_ALWAYS)
            }
            (false, true) => Some(self.saver_threshold.take().unwrap_or(SAVER_DEFAULT)),
            _ => None,
        }
    }
    pub fn travel_on(&mut self, current: Current) -> Changes {
        if self.travel.is_some() {
            return Changes::default();
        }
        self.travel = Some(Travel {
            mode: current.mode,
            brightness: current.brightness,
            saver_forced: current.saver_threshold == Some(SAVER_ALWAYS),
        });
        Changes {
            mode: current.mode.map(|_| Mode::Saver),
            brightness: current
                .brightness
                .filter(|b| *b > TRAVEL_BRIGHTNESS)
                .map(|_| TRAVEL_BRIGHTNESS),
            saver_threshold: self.saver(true, current.saver_threshold),
        }
    }
    pub fn travel_off(&mut self, current: Current) -> Changes {
        let Some(travel) = self.travel.take() else {
            return Changes::default();
        };
        Changes {
            mode: travel.mode,
            brightness: travel.brightness,
            saver_threshold: if travel.saver_forced {
                None
            } else {
                self.saver(false, current.saver_threshold)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions() {
        assert_eq!(parse("refresh").unwrap(), Action::Refresh);
        assert_eq!(parse("").unwrap(), Action::Refresh);
        assert_eq!(parse("mode saver").unwrap(), Action::Mode(Mode::Saver));
        assert_eq!(
            parse("mode performance").unwrap(),
            Action::Mode(Mode::Performance)
        );
        assert_eq!(parse("brightness 55").unwrap(), Action::Brightness(55));
        assert_eq!(parse("brightness 180").unwrap(), Action::Brightness(100));
        assert_eq!(parse("brightness -3").unwrap(), Action::Brightness(0));
        assert_eq!(parse("saver on").unwrap(), Action::Saver(true));
        assert_eq!(parse("travel off").unwrap(), Action::Travel(false));
        for bad in [
            "mode",
            "mode turbo",
            "brightness",
            "brightness x",
            "saver",
            "travel maybe",
            "refresh now",
            "up",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn power_modes_round_trip_and_accept_the_windows_10_identifier() {
        for mode in [Mode::Saver, Mode::Balanced, Mode::Performance] {
            assert_eq!(Mode::from_overlay(mode.overlay()), Some(mode));
        }
        assert_eq!(
            Mode::from_overlay(OVERLAY_BALANCED_LEGACY),
            Some(Mode::Balanced)
        );
        assert_eq!(Mode::from_overlay(1), None);
    }
    fn reading() -> Reading {
        Reading {
            percent: 50,
            design_mwh: Some(80_000),
            full_mwh: Some(67_090),
            remaining_mwh: Some(33_545),
            ..Default::default()
        }
    }
    #[test]
    fn health_rounds_and_needs_both_capacities() {
        assert_eq!(reading().health(), Some(84));
        assert_eq!(
            Reading {
                design_mwh: None,
                ..reading()
            }
            .health(),
            None
        );
        assert_eq!(
            Reading {
                design_mwh: Some(0),
                ..reading()
            }
            .health(),
            None
        );
        assert_eq!(
            Reading {
                full_mwh: Some(83_000),
                ..reading()
            }
            .health(),
            Some(104)
        );
    }
    #[test]
    fn states() {
        let plugged = |percent, charging| Reading {
            plugged: true,
            charging,
            percent,
            ..reading()
        };
        assert_eq!(plugged(50, true).state(), "Charging");
        assert_eq!(plugged(100, false).state(), "Full");
        assert_eq!(plugged(80, false).state(), "Not charging");
        assert_eq!(reading().state(), "On battery");
    }
    #[test]
    fn remaining_time_prefers_windows_estimate_and_falls_back_to_the_rate() {
        let charging = Reading {
            plugged: true,
            charging: true,
            rate_mw: Some(33_545),
            ..reading()
        };
        assert_eq!(charging.time(), "1h00 to full");
        assert_eq!(charging.power(), "33.5 W");
        let draining = Reading {
            rate_mw: Some(-8_000),
            ..reading()
        };
        assert_eq!(draining.time(), "4h11 left");
        assert_eq!(draining.power(), "8.0 W");
        assert_eq!(
            Reading {
                lifetime_s: Some(3_900),
                ..draining.clone()
            }
            .time(),
            "1h05 left"
        );
        assert_eq!(
            Reading {
                rate_mw: None,
                ..draining
            }
            .time(),
            ""
        );
        assert_eq!(
            Reading {
                plugged: true,
                ..reading()
            }
            .time(),
            ""
        );
        assert_eq!(
            Reading {
                plugged: true,
                charging: true,
                rate_mw: Some(0),
                ..reading()
            }
            .time(),
            ""
        );
        assert_eq!(watt_hours(Some(67_090)), "67.1 Wh");
        assert_eq!(watt_hours(None), "");
    }
    #[test]
    fn stats_list_only_what_the_battery_reports() {
        let full = Reading {
            cycles: Some(126),
            rate_mw: Some(-8_000),
            ..reading()
        };
        assert_eq!(
            stats(&full),
            [
                ("Design capacity", "80.0 Wh".to_owned()),
                ("Full charge", "67.1 Wh".to_owned()),
                ("Health", "84%".to_owned()),
                ("Cycles", "126".to_owned()),
                ("Power", "8.0 W".to_owned()),
            ]
        );
        let relative = Reading {
            percent: 70,
            ..Default::default()
        };
        assert!(stats(&relative).is_empty());
    }
    #[test]
    fn travel_summary_names_available_controls() {
        assert_eq!(
            travel_summary(true, true, true),
            "Power saver mode, 40% brightness at most and battery saver"
        );
        assert_eq!(
            travel_summary(true, false, true),
            "Power saver mode and battery saver"
        );
        assert_eq!(travel_summary(false, true, false), "40% brightness at most");
        assert_eq!(travel_summary(false, false, false), "");
    }
    #[test]
    fn saver_toggle_restores_the_previous_threshold() {
        let mut memory = Memory::default();
        assert_eq!(memory.saver(true, Some(30)), Some(SAVER_ALWAYS));
        assert_eq!(memory.saver(true, Some(SAVER_ALWAYS)), None);
        assert_eq!(memory.saver(false, Some(SAVER_ALWAYS)), Some(30));
        assert_eq!(memory, Memory::default());
        assert_eq!(memory.saver(false, Some(SAVER_ALWAYS)), Some(SAVER_DEFAULT));
        assert_eq!(memory.saver(false, Some(20)), None);
        assert_eq!(memory.saver(true, None), None);
    }
    #[test]
    fn travel_mode_applies_and_restores_only_available_controls() {
        let before = Current {
            mode: Some(Mode::Performance),
            brightness: Some(90),
            saver_threshold: Some(20),
        };
        let mut memory = Memory::default();
        assert_eq!(
            memory.travel_on(before),
            Changes {
                mode: Some(Mode::Saver),
                brightness: Some(40),
                saver_threshold: Some(SAVER_ALWAYS)
            }
        );
        assert_eq!(memory.travel_on(before), Changes::default());
        let during = Current {
            mode: Some(Mode::Saver),
            brightness: Some(40),
            saver_threshold: Some(SAVER_ALWAYS),
        };
        assert_eq!(
            memory.travel_off(during),
            Changes {
                mode: Some(Mode::Performance),
                brightness: Some(90),
                saver_threshold: Some(20)
            }
        );
        assert_eq!(memory, Memory::default());
        assert_eq!(memory.travel_off(during), Changes::default());

        let desktop = Current {
            mode: Some(Mode::Balanced),
            brightness: None,
            saver_threshold: None,
        };
        assert_eq!(
            memory.travel_on(desktop),
            Changes {
                mode: Some(Mode::Saver),
                brightness: None,
                saver_threshold: None
            }
        );
    }
    #[test]
    fn travel_mode_keeps_a_saver_the_user_had_forced_and_never_brightens() {
        let mut memory = Memory::default();
        let before = Current {
            mode: None,
            brightness: Some(25),
            saver_threshold: Some(SAVER_ALWAYS),
        };
        assert_eq!(memory.travel_on(before), Changes::default());
        assert_eq!(
            memory.travel_off(before),
            Changes {
                mode: None,
                brightness: Some(25),
                saver_threshold: None
            }
        );
    }
    #[test]
    fn memory_survives_a_restart_and_an_empty_one_removes_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("battery.json");
        assert_eq!(Memory::load(&path), Memory::default());
        let mut memory = Memory::default();
        memory.travel_on(Current {
            mode: Some(Mode::Balanced),
            brightness: Some(70),
            saver_threshold: Some(20),
        });
        memory.save(&path).unwrap();
        assert_eq!(Memory::load(&path), memory);
        Memory::default().save(&path).unwrap();
        assert!(!path.exists());
        Memory::default().save(&path).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Memory::load(&path), Memory::default());
    }
}
