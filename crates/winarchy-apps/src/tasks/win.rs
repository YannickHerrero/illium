//! Process sampling and termination through Toolhelp and the process handles.
use super::model::Process;
use std::collections::HashMap;
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::{
        Diagnostics::ToolHelp::*,
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::*,
    },
};
fn ticks(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}
/// Keeps the previous CPU times so each sample yields a rate.
#[derive(Default)]
pub struct Sampler {
    previous: HashMap<u32, u64>,
    at: Option<std::time::Instant>,
}
impl Sampler {
    pub fn sample(&mut self) -> Vec<Process> {
        let now = std::time::Instant::now();
        let elapsed = self.at.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        let cpus = std::thread::available_parallelism().map_or(1.0, |n| n.get() as f64);
        let mut current = HashMap::new();
        let mut out = Vec::new();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more {
                let pid = entry.th32ProcessID;
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if pid != 0 {
                    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok();
                    let (cpu_ticks, memory) = handle.map_or((None, 0), |h| {
                        let r = (times(h), working_set(h));
                        let _ = CloseHandle(h);
                        r
                    });
                    let cpu = match (cpu_ticks, self.previous.get(&pid)) {
                        (Some(t), Some(&p)) if elapsed > 0.0 => {
                            // FILETIME ticks are 100 ns.
                            (t.saturating_sub(p) as f64 / 1e7 / elapsed / cpus * 100.0) as f32
                        }
                        _ => 0.0,
                    };
                    if let Some(t) = cpu_ticks {
                        current.insert(pid, t);
                    }
                    out.push(Process {
                        pid,
                        name: if name.is_empty() {
                            "(unknown)".into()
                        } else {
                            name
                        },
                        cpu,
                        memory,
                        accessible: handle.is_some(),
                    });
                }
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
        }
        self.previous = current;
        self.at = Some(now);
        out
    }
}
fn times(h: HANDLE) -> Option<u64> {
    let mut f = [FILETIME::default(); 4];
    unsafe { GetProcessTimes(h, &mut f[0], &mut f[1], &mut f[2], &mut f[3]) }.ok()?;
    Some(ticks(f[2]) + ticks(f[3]))
}
fn working_set(h: HANDLE) -> u64 {
    let mut c = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    if unsafe { K32GetProcessMemoryInfo(h, &mut c, c.cb) }.as_bool() {
        c.WorkingSetSize as u64
    } else {
        0
    }
}
pub fn terminate(pid: u32) -> Result<(), String> {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| e.message())?;
        let r = TerminateProcess(h, 1).map_err(|e| e.message());
        let _ = CloseHandle(h);
        r
    }
}
