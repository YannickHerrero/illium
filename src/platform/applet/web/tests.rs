//! Opt-in: drives a real WebView2 on the bundled web template. Never runs the
//! provider; the page's network probes are expected to be refused.
use super::*;
use std::time::{Duration, Instant};
use webview2_com::ExecuteScriptCompletedHandler;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out: {what}"
        );
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn eval(view: &View, script: &str) -> String {
    let web = view.0.borrow().web.clone().expect("controller ready");
    let slot = Rc::new(RefCell::new(None));
    let result = slot.clone();
    unsafe {
        web.ExecuteScript(
            &HSTRING::from(script),
            &ExecuteScriptCompletedHandler::create(Box::new(move |status, json| {
                *result.borrow_mut() = Some(status.map(|()| json));
                Ok(())
            })),
        )
        .unwrap();
    }
    pump_until(script, || slot.borrow().is_some());
    slot.take().unwrap().unwrap()
}
fn next_action(rx: &std::sync::mpsc::Receiver<Event>) -> Option<String> {
    let mut action = None;
    pump_until("page action", || {
        while let Ok(event) = rx.try_recv() {
            if let Event::AppletAction(name, 7, value) = event {
                assert_eq!(name, "_web-template");
                action = Some(value);
            }
        }
        action.is_some()
    });
    action.unwrap()
}
#[test]
#[ignore = "requires the WebView2 runtime and a Windows desktop session"]
fn web_template_is_confined_and_talks_to_illium() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let dir = std::env::temp_dir().join(format!("illium-web-applet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [
        (
            "applet.toml",
            include_str!("../../../../config/applets/_web-template/applet.toml"),
        ),
        (
            "index.html",
            include_str!("../../../../config/applets/_web-template/index.html"),
        ),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let applet = Applet {
        name: "_web-template".into(),
        manifest: toml::from_str(&std::fs::read_to_string(dir.join("applet.toml")).unwrap())
            .unwrap(),
        dir: dir.clone(),
    };
    let theme = crate::config::Theme::parse(
        "name = 't'\nbackground = '#1e1e2e'\nsurface = '#313244'\noverlay = '#45475a'\n\
         text = '#cdd6f4'\nsubtext = '#a6adc8'\naccent = '#cba6f7'\ngreen = '#a6e3a1'\n\
         yellow = '#f9e2af'\nred = '#f38ba8'\n",
    )
    .unwrap();
    let (tx, rx) = crate::queue::channel(8);
    let view = View::new(&applet, "index.html", 7, &theme, &tx).unwrap();
    view.data(&json!({"message": "Hello from the test", "count": 3, "action": ""}));
    pump_until("page ready", || {
        let state = view.0.borrow();
        assert!(state.failure.is_none(), "{:?}", state.failure);
        state.ready
    });
    pump_until("data rendered", || {
        eval(&view, "document.getElementById('message').textContent") == r#""Hello from the test""#
    });
    assert_eq!(
        eval(
            &view,
            "getComputedStyle(document.documentElement).getPropertyValue('--illium-bg')"
        ),
        r##""#1e1e2e""##
    );
    assert_eq!(
        eval(&view, "matchMedia('(prefers-color-scheme: dark)').matches"),
        "true"
    );
    assert_eq!(
        eval(&view, "location.origin"),
        r#""https://applet--web-template.illium.example""#
    );

    eval(&view, "illium.action(['bar', '141,0,0'])");
    assert_eq!(next_action(&rx).as_deref(), Some(r#"["bar","141,0,0"]"#));
    eval(&view, "illium.action()");
    assert_eq!(next_action(&rx), None);

    eval(
        &view,
        "fetch('https://example.com/').then(r => illium.action('status ' + r.status), () => illium.action('refused'))",
    );
    let fetched = next_action(&rx).unwrap();
    assert!(fetched == "refused" || fetched == "status 403", "{fetched}");
    eval(
        &view,
        "fetch('applet.toml').then(r => illium.action('local ' + r.status), () => illium.action('local refused'))",
    );
    assert_eq!(next_action(&rx).as_deref(), Some("local 200"));

    eval(&view, "location.href = 'https://example.com/'");
    std::thread::sleep(Duration::from_millis(500));
    pump_until("navigation settled", || true);
    assert_eq!(
        eval(&view, "location.origin"),
        r#""https://applet--web-template.illium.example""#
    );
    assert_eq!(
        eval(&view, "window.open('https://example.com/') === null"),
        "true"
    );

    // A hidden page must not close the open popup of another applet.
    eval(&view, "illium.close()");
    pump_until("close ignored", || true);
    assert!(rx.try_recv().is_err());

    drop(view);
    let _ = std::fs::remove_dir_all(dir);
}
