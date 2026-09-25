//! Opt-in desktop probe. Start `illium-terminal --serve` first. Opens/closes
//! only its own new windows; never stops WSL or changes the user's theme.
#[cfg(windows)]
fn main() -> Result<(), String> {
    use illium_ipc::{client, identity};
    use std::time::{Duration, Instant};
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    struct Search {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn enumerate(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        unsafe {
            let search = &mut *(data.0 as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == search.pid && IsWindowVisible(hwnd).as_bool() {
                search.windows.push(hwnd);
            }
            true.into()
        }
    }
    fn windows(pid: u32) -> Vec<HWND> {
        let mut search = Search {
            pid,
            windows: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(enumerate), LPARAM(&mut search as *mut Search as isize));
        }
        search.windows
    }
    let count = std::env::args()
        .nth(1)
        .unwrap_or("20".into())
        .parse::<usize>()
        .map_err(|e| e.to_string())?
        .clamp(1, 100);
    let pipe = client::pipe_path(&identity::endpoint_named("illium-terminal")?);
    let request = |line: &str| {
        let r = client::client_at(&pipe, line, Duration::from_secs(5))?;
        if r.ok { Ok(r.message) } else { Err(r.message) }
    };
    let pid = request("pid")?.parse::<u32>().map_err(|e| e.to_string())?;
    let mut samples = Vec::new();
    println!("iteration,visible_ms,ack_ms");
    for i in 0..count {
        let before = windows(pid);
        let at = Instant::now();
        // Same two-message path as the Illium hotkey, including focus grant.
        request("pid")?;
        unsafe {
            let _ = AllowSetForegroundWindow(pid);
        }
        request("open")?;
        let ack = at.elapsed().as_secs_f64() * 1000.;
        let hwnd = loop {
            if let Some(h) = windows(pid).into_iter().find(|h| !before.contains(h)) {
                break h;
            }
            if at.elapsed() > Duration::from_secs(5) {
                return Err("No new terminal window".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let elapsed = at.elapsed().as_secs_f64() * 1000.;
        samples.push(elapsed);
        println!("{},{elapsed:.2},{ack:.2}", i + 1);
        std::thread::sleep(Duration::from_millis(500));
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .map_err(|e| e.to_string())?;
        let close = Instant::now();
        while windows(pid).contains(&hwnd) {
            if close.elapsed() > Duration::from_secs(5) {
                return Err("Window did not close".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "p50_ms={:.2} p95_ms={:.2} count={count}",
        samples[(count as f64 * 0.5).ceil() as usize - 1],
        samples[(count as f64 * 0.95).ceil() as usize - 1]
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This desktop probe requires Windows");
}
