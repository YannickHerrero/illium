//! Windows side of the `builtin:battery` provider: battery readings from the
//! battery class driver, the power mode, the battery saver threshold and the
//! brightness of the built-in display. Every control is optional: a machine
//! without one reports it as unavailable and the view hides it.
use crate::battery::{self, Action, Current, Memory, Mode, Reading};
use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::*,
        Foundation::{CloseHandle, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HLOCAL, LocalFree},
        Storage::FileSystem::*,
        System::{Com::*, IO::DeviceIoControl, Power::*, Variant::VARIANT, Wmi::*},
    },
    core::{BSTR, GUID, PCWSTR},
};

// Windows exports these behind its power mode setting without declaring
// them in the SDK headers or import library; stable since Windows 10 1709.
#[link(name = "powrprof", kind = "raw-dylib")]
unsafe extern "system" {
    fn PowerGetEffectiveOverlayScheme(overlay: *mut GUID) -> u32;
    fn PowerSetActiveOverlayScheme(overlay: GUID) -> u32;
}
/// Windows only offers the power mode setting while this plan is active.
const SCHEME_BALANCED: GUID = GUID::from_u128(0x381b4222_f694_41f0_9685_ff5bb260df2e);
const SUB_ENERGYSAVER: GUID = GUID::from_u128(0xde830923_a562_41af_a086_e3a2c6bad2da);
const ESBATTTHRESHOLD: GUID = GUID::from_u128(0xe69653ca_cf7f_4f05_aa73_cb833fa90ad4);

fn memory_path() -> std::path::PathBuf {
    crate::config::Config::home().join("battery.json")
}

struct Device {
    info: BATTERY_INFORMATION,
    status: BATTERY_STATUS,
}
/// Every present battery, read through the class driver like `powercfg /batteryreport`.
fn devices() -> Vec<Device> {
    let mut out = Vec::new();
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(
            Some(&GUID_DEVICE_BATTERY),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        ) else {
            return out;
        };
        for index in 0..8 {
            let mut interface = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVICE_BATTERY, index, &mut interface)
                .is_err()
            {
                break;
            }
            let mut required = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(
                set,
                &interface,
                None,
                0,
                Some(&mut required),
                None,
            );
            if required == 0 {
                continue;
            }
            // u32 storage keeps the detail structure aligned.
            let mut buffer = vec![0u32; (required as usize).div_ceil(4)];
            let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &interface, Some(detail), required, None, None)
                .is_err()
            {
                continue;
            }
            if let Some(device) = read_device(PCWSTR((*detail).DevicePath.as_ptr())) {
                out.push(device);
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}
unsafe fn read_device(path: PCWSTR) -> Option<Device> {
    unsafe {
        let handle = CreateFileW(
            path,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        let read = || -> Option<Device> {
            let wait = 0u32;
            let mut tag = 0u32;
            DeviceIoControl(
                handle,
                IOCTL_BATTERY_QUERY_TAG,
                Some(&wait as *const u32 as _),
                4,
                Some(&mut tag as *mut u32 as _),
                4,
                None,
                None,
            )
            .ok()?;
            if tag == 0 {
                return None;
            }
            let query = BATTERY_QUERY_INFORMATION {
                BatteryTag: tag,
                InformationLevel: BatteryInformation,
                AtRate: 0,
            };
            let mut info = BATTERY_INFORMATION::default();
            DeviceIoControl(
                handle,
                IOCTL_BATTERY_QUERY_INFORMATION,
                Some(&query as *const _ as _),
                size_of_val(&query) as u32,
                Some(&mut info as *mut _ as _),
                size_of_val(&info) as u32,
                None,
                None,
            )
            .ok()?;
            let wait = BATTERY_WAIT_STATUS {
                BatteryTag: tag,
                ..Default::default()
            };
            let mut status = BATTERY_STATUS::default();
            DeviceIoControl(
                handle,
                IOCTL_BATTERY_QUERY_STATUS,
                Some(&wait as *const _ as _),
                size_of_val(&wait) as u32,
                Some(&mut status as *mut _ as _),
                size_of_val(&status) as u32,
                None,
                None,
            )
            .ok()?;
            Some(Device { info, status })
        };
        let device = read();
        let _ = CloseHandle(handle);
        device
    }
}
fn power_status() -> Option<SYSTEM_POWER_STATUS> {
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    (status.BatteryLifePercent <= 100).then_some(status)
}
fn reading(status: &SYSTEM_POWER_STATUS) -> Reading {
    let devices = devices();
    // Relative capacities are percentages, not mWh: nothing to show in Wh.
    let absolute = !devices.is_empty()
        && devices
            .iter()
            .all(|d| d.info.Capabilities & BATTERY_CAPACITY_RELATIVE == 0);
    let sum = |value: fn(&Device) -> u32| -> Option<u32> {
        let values: Vec<u32> = devices.iter().map(value).collect();
        (absolute
            && values
                .iter()
                .all(|v| *v != 0 && *v != BATTERY_UNKNOWN_CAPACITY))
        .then(|| values.iter().sum())
    };
    let rates: Vec<i32> = devices.iter().map(|d| d.status.Rate).collect();
    Reading {
        percent: status.BatteryLifePercent,
        plugged: status.ACLineStatus == 1,
        charging: if devices.is_empty() {
            status.BatteryFlag & 8 != 0
        } else {
            devices
                .iter()
                .any(|d| d.status.PowerState & BATTERY_CHARGING != 0)
        },
        design_mwh: sum(|d| d.info.DesignedCapacity),
        full_mwh: sum(|d| d.info.FullChargedCapacity),
        remaining_mwh: sum(|d| d.status.Capacity),
        rate_mw: (absolute && rates.iter().all(|r| *r as u32 != BATTERY_UNKNOWN_RATE))
            .then(|| rates.iter().sum()),
        // Batteries that do not count cycles report zero.
        cycles: devices
            .iter()
            .map(|d| d.info.CycleCount)
            .max()
            .filter(|c| *c > 0),
        lifetime_s: (status.BatteryLifeTime != u32::MAX).then_some(status.BatteryLifeTime),
    }
}

fn active_scheme() -> Option<GUID> {
    unsafe {
        let mut scheme: *mut GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(None, &mut scheme) != ERROR_SUCCESS || scheme.is_null() {
            return None;
        }
        let guid = *scheme;
        let _ = LocalFree(Some(HLOCAL(scheme as _)));
        Some(guid)
    }
}
fn mode(scheme: Option<GUID>) -> Option<Mode> {
    if scheme != Some(SCHEME_BALANCED) {
        return None;
    }
    let mut overlay = GUID::zeroed();
    (unsafe { PowerGetEffectiveOverlayScheme(&mut overlay) } == 0)
        .then(|| Mode::from_overlay(overlay.to_u128()))
        .flatten()
}
fn set_mode(mode: Mode) -> Result<(), String> {
    match unsafe { PowerSetActiveOverlayScheme(GUID::from_u128(mode.overlay())) } {
        0 => Ok(()),
        code => Err(format!("cannot change the power mode (error {code})")),
    }
}
fn saver_threshold(scheme: Option<GUID>) -> Option<u32> {
    let scheme = scheme?;
    let mut value = 0u32;
    (unsafe {
        PowerReadDCValueIndex(
            None,
            Some(&scheme),
            Some(&SUB_ENERGYSAVER),
            Some(&ESBATTTHRESHOLD),
            &mut value,
        )
    } == 0)
        .then_some(value)
}
fn set_saver_threshold(scheme: Option<GUID>, value: u32) -> Result<(), String> {
    let scheme = scheme.ok_or("no active power plan")?;
    unsafe {
        let code = PowerWriteDCValueIndex(
            None,
            &scheme,
            Some(&SUB_ENERGYSAVER),
            Some(&ESBATTTHRESHOLD),
            value,
        );
        if code != 0 {
            return Err(format!("cannot change the battery saver (error {code})"));
        }
        // Written values only take effect once the plan is applied again.
        PowerSetActiveScheme(None, Some(&scheme))
            .ok()
            .map_err(|e| format!("cannot apply the battery saver: {e}"))
    }
}

/// The built-in display, through the WMI classes its driver provides;
/// external monitors and desktops have none.
struct Display {
    services: IWbemServices,
}
impl Display {
    fn connect() -> Option<Self> {
        unsafe {
            let locator: IWbemLocator =
                CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
            let services = locator
                .ConnectServer(
                    &BSTR::from("root\\wmi"),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
                .ok()?;
            // RPC_C_AUTHN_WINNT and RPC_C_AUTHZ_NONE: the local WMI service
            // refuses calls made at the default, unauthenticated level.
            CoSetProxyBlanket(
                &services,
                10,
                0,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
            .ok()?;
            Some(Self { services })
        }
    }
    fn first(&self, class: &str) -> Option<IWbemClassObject> {
        unsafe {
            let rows = self
                .services
                .ExecQuery(
                    &BSTR::from("WQL"),
                    &BSTR::from(format!("SELECT * FROM {class} WHERE Active = TRUE")),
                    WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                    None,
                )
                .ok()?;
            let mut row = [None];
            let mut returned = 0;
            let _ = rows.Next(WBEM_INFINITE, &mut row, &mut returned);
            row[0].take()
        }
    }
    fn brightness(&self) -> Option<u8> {
        let row = self.first("WmiMonitorBrightness")?;
        let mut value = VARIANT::default();
        unsafe {
            row.Get(
                windows::core::w!("CurrentBrightness"),
                0,
                &mut value,
                None,
                None,
            )
        }
        .ok()?;
        u32::try_from(&value).ok().map(|v| v.min(100) as u8)
    }
    fn set_brightness(&self, level: u8) -> Result<(), String> {
        let unavailable = || "no adjustable built-in display".to_owned();
        unsafe {
            let target = self
                .first("WmiMonitorBrightnessMethods")
                .ok_or_else(unavailable)?;
            let mut path = VARIANT::default();
            target
                .Get(windows::core::w!("__PATH"), 0, &mut path, None, None)
                .map_err(|e| e.to_string())?;
            let path = BSTR::try_from(&path).map_err(|e| e.to_string())?;
            let mut class = None;
            self.services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let class: IWbemClassObject = class.ok_or_else(unavailable)?;
            let mut signature = None;
            class
                .GetMethod(
                    windows::core::w!("WmiSetBrightness"),
                    0,
                    &mut signature,
                    std::ptr::null_mut(),
                )
                .map_err(|e| e.to_string())?;
            let input = signature
                .ok_or_else(unavailable)?
                .SpawnInstance(0)
                .map_err(|e| e.to_string())?;
            input
                .Put(windows::core::w!("Timeout"), 0, &VARIANT::from(0i32), 0)
                .map_err(|e| e.to_string())?;
            input
                .Put(
                    windows::core::w!("Brightness"),
                    0,
                    &VARIANT::from(i32::from(level)),
                    0,
                )
                .map_err(|e| e.to_string())?;
            self.services
                .ExecMethod(
                    &path,
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    &input,
                    None,
                    None,
                )
                .map_err(|e| format!("cannot change the brightness: {e}"))
        }
    }
}

fn with_com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _apartment = Apartment;
    f()
}
fn apply(action: Action, display: Option<&Display>) -> Result<(), String> {
    let scheme = active_scheme();
    let current = || Current {
        mode: mode(scheme),
        brightness: display.and_then(Display::brightness),
        saver_threshold: saver_threshold(scheme),
    };
    let write = |changes: battery::Changes| -> Result<(), String> {
        if let Some(mode) = changes.mode {
            set_mode(mode)?;
        }
        if let (Some(level), Some(display)) = (changes.brightness, display) {
            display.set_brightness(level)?;
        }
        if let Some(threshold) = changes.saver_threshold {
            set_saver_threshold(scheme, threshold)?;
        }
        Ok(())
    };
    match action {
        Action::Refresh => Ok(()),
        Action::Mode(mode) => set_mode(mode),
        Action::Brightness(level) => display
            .ok_or("no adjustable built-in display")?
            .set_brightness(level),
        Action::Saver(on) => {
            let mut memory = Memory::load(&memory_path());
            if let Some(threshold) = memory.saver(on, saver_threshold(scheme)) {
                set_saver_threshold(scheme, threshold)?;
            }
            memory.save(&memory_path())
        }
        Action::Travel(on) => {
            let mut memory = Memory::load(&memory_path());
            let changes = if on {
                memory.travel_on(current())
            } else {
                memory.travel_off(current())
            };
            // Remembered first, so a failed write can still be undone.
            memory.save(&memory_path())?;
            write(changes)
        }
    }
}
/// Worker-only provider entry: runs the view's action, then reads everything.
pub fn query(action: Option<&str>) -> Result<String, String> {
    with_com(|| {
        let action = battery::parse(action.unwrap_or_default())?;
        let display = Display::connect();
        let applied = apply(action, display.as_ref());
        let Some(status) = power_status() else {
            return Ok(serde_json::json!({ "present": false }).to_string());
        };
        let reading = reading(&status);
        let scheme = active_scheme();
        let threshold = saver_threshold(scheme);
        let mode = mode(scheme);
        let brightness = display.as_ref().and_then(Display::brightness);
        let stats: Vec<_> = battery::stats(&reading)
            .into_iter()
            .map(|(label, value)| serde_json::json!({ "label": label, "value": value }))
            .collect();
        let mut data = serde_json::json!({
            "present": true,
            "percent": reading.percent,
            "plugged": reading.plugged,
            "state": reading.state(),
            "time": reading.time(),
            "power": reading.power(),
            "stats": stats,
            "mode": mode.map_or("", Mode::name),
            "saver_active": status.SystemStatusFlag == 1,
            "saver_threshold": threshold.map_or(-1, i64::from),
            "saver_forced": threshold == Some(battery::SAVER_ALWAYS),
            "brightness": brightness.map_or(-1, i64::from),
            "travel": Memory::load(&memory_path()).travel.is_some(),
            "travel_summary": battery::travel_summary(
                mode.is_some(),
                brightness.is_some(),
                threshold.is_some(),
            ),
            "error": "",
        });
        if let Err(error) = applied {
            data["error"] = error.into();
        }
        Ok(data.to_string())
    })
}
#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "reads the real battery, power plan and display; read-only"]
    fn reads_this_machine() {
        let data: serde_json::Value =
            serde_json::from_str(&super::query(Some("refresh")).unwrap()).unwrap();
        println!("{data:#}");
        assert_eq!(data["error"], "");
    }
}
