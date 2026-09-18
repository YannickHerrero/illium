//! Windows-only regression using the real host loop and controls, without
//! injecting keyboard/mouse input or exposing a production test command.
use super::*;

#[derive(Default)]
struct Probe {
    stage: usize,
    ticks: usize,
    completed: bool,
    error: Option<String>,
}
thread_local! { static PROBE: RefCell<Probe> = RefCell::new(Probe::default()); }
const PROBE_TIMER: usize = 0x545354;

#[test]
#[ignore = "requires Windows and WebView2; run alone with --test-threads=1"]
fn demo_home_never_loads_the_normal_library() {
    struct ConfigGuard(Option<std::ffi::OsString>);
    impl Drop for ConfigGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(value) = &self.0 {
                    std::env::set_var("WINARCHY_CONFIG_HOME", value);
                } else {
                    std::env::remove_var("WINARCHY_CONFIG_HOME");
                }
            }
        }
    }
    let _restore = ConfigGuard(std::env::var_os("WINARCHY_CONFIG_HOME"));
    let normal = tempfile::tempdir().unwrap();
    std::fs::create_dir(normal.path().join("browser")).unwrap();
    // Invalid JSON: opening the normal library would fail, even without a leak.
    let personal = normal.path().join("browser/library.json");
    std::fs::write(&personal, "DO NOT READ: personal library sentinel").unwrap();
    unsafe {
        std::env::set_var("WINARCHY_CONFIG_HOME", normal.path());
    }
    let demo = winarchy_browser::demo::DemoData::new().unwrap();
    let demo_path = demo.path().to_owned();
    let mut checked = false;
    let result = run_inner(
        "",
        true,
        None,
        |_| {
            let app = snapshot().unwrap();
            assert!(app.home);
            let picker = app.picker.borrow();
            let rows = picker.library.borrow().suggestions("");
            assert_eq!(rows.len(), 5);
            assert!(
                rows.iter()
                    .any(|row| row.site.title == "Rust Programming Language")
            );
            checked = true;
            unsafe {
                let _ = PostMessageW(Some(app.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        },
        Some(demo.path()),
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(checked);
    assert_eq!(
        std::fs::read_to_string(personal).unwrap(),
        "DO NOT READ: personal library sentinel"
    );
    drop(demo);
    assert!(!demo_path.exists(), "temporary profile was not cleaned up");
}

unsafe fn tick(hwnd: HWND) {
    let (stage, ticks) = PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        probe.ticks += 1;
        (probe.stage, probe.ticks)
    });
    let Some(app) = snapshot() else {
        return;
    };
    let (visible, tabs_mode, panel, edit, list) = {
        let picker = app.picker.borrow();
        (
            picker.visible,
            picker.tabs_mode,
            picker.panel,
            picker.edit,
            picker.list,
        )
    };
    let count = SendMessageW(list, LB_GETCOUNT, None, None).0;
    let mut advance = false;
    match stage {
        0 if visible && tabs_mode && count == 2 => {
            let mut title = [0u16; 128];
            let length = GetWindowTextW(panel, &mut title) as usize;
            let title = String::from_utf16_lossy(&title[..length]);
            if title != "Open tabs" {
                PROBE.with(|probe| {
                    probe.borrow_mut().error =
                        Some(format!("Expected English tab palette title, got {title:?}"))
                });
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                return;
            }
            // Exercise queued query refreshes after inactive badges have painted.
            let _ = SetWindowTextW(edit, w!("no-tab-can-match-this"));
            advance = true;
        }
        1 if visible && tabs_mode && count == 0 => {
            let _ = SetWindowTextW(edit, w!(""));
            advance = true;
        }
        2 if visible && tabs_mode && count == 2 => {
            queue_action(Action::CloseTab);
            advance = true;
        }
        3 if visible && tabs_mode && count == 1 => {
            // Opening an already-visible tab palette must remain reentrant-safe.
            queue_action(Action::SelectTab);
            advance = true;
        }
        4 if visible && tabs_mode && count == 1 => {
            queue_action(Action::CloseTab);
            advance = true;
        }
        5 if visible && !tabs_mode && app.tabs.borrow().entries().len() == 1 => {
            queue_action(Action::SelectTab);
            advance = true;
        }
        6 if visible && tabs_mode && count == 1 => {
            PROBE.with(|probe| probe.borrow_mut().completed = true);
            let _ = KillTimer(Some(hwnd), PROBE_TIMER);
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        _ => {}
    }
    if advance {
        PROBE.with(|probe| probe.borrow_mut().stage += 1);
    }
    if ticks >= 100 {
        PROBE.with(|probe| probe.borrow_mut().error = Some(format!("Palette stalled at stage {stage}: visible={visible}, tabs={tabs_mode}, rows={count}")));
        let _ = KillTimer(Some(hwnd), PROBE_TIMER);
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
unsafe extern "system" fn timer(hwnd: HWND, _: u32, _: usize, _: u32) {
    tick(hwnd);
}

#[test]
#[ignore = "requires an interactive Windows desktop and the WebView2 runtime"]
fn tab_palette_open_filter_close_and_reopen_on_windows() {
    unsafe {
        let old_config = std::env::var_os("WINARCHY_CONFIG_HOME");
        let old_local = std::env::var_os("LOCALAPPDATA");
        let root = std::env::temp_dir().join(format!(
            "winarchy-palette-regression-{}-{}",
            std::process::id(),
            tabs::now()
        ));
        let config = root.join("config");
        std::fs::create_dir_all(config.join("browser")).unwrap();
        std::fs::write(
            config.join("browser/custom.txt"),
            "! isolated palette regression\n",
        )
        .unwrap();
        std::env::set_var("WINARCHY_CONFIG_HOME", &config);
        std::env::set_var("LOCALAPPDATA", root.join("local"));
        PROBE.with(|probe| *probe.borrow_mut() = Probe::default());
        let result = run("", false, None, |_| {
            let hwnd = snapshot().unwrap().hwnd;
            queue_action(Action::NewTab);
            queue_action(Action::SelectTab);
            SetTimer(Some(hwnd), PROBE_TIMER, 100, Some(timer));
        });
        match old_config {
            Some(value) => std::env::set_var("WINARCHY_CONFIG_HOME", value),
            None => std::env::remove_var("WINARCHY_CONFIG_HOME"),
        }
        match old_local {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
        assert!(
            result.is_ok(),
            "Browser failed: {:?}; logs/profile: {}",
            result.err(),
            root.display()
        );
        PROBE.with(|probe| {
            let probe = probe.borrow();
            assert!(
                probe.completed && probe.error.is_none(),
                "{:?}; logs/profile: {}",
                probe.error,
                root.display()
            );
        });
    }
}
