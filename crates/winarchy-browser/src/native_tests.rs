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

unsafe fn tick(hwnd: HWND) {
    let (stage, ticks) = PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        probe.ticks += 1;
        (probe.stage, probe.ticks)
    });
    let Some(app) = snapshot() else {
        return;
    };
    let (visible, tabs_mode, edit, list) = {
        let picker = app.picker.borrow();
        (picker.visible, picker.tabs_mode, picker.edit, picker.list)
    };
    let count = SendMessageW(list, LB_GETCOUNT, None, None).0;
    let mut advance = false;
    match stage {
        0 if visible && tabs_mode && count == 2 => {
            // The queued query refresh must not reenter a borrowed picker.
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
