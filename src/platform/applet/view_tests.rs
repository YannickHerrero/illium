//! Opt-in headless rendering/interaction test; never executes a provider or changes Wi-Fi.
use super::*;
use slint::Rgb8Pixel;
use slint::platform::{
    Platform, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{cell::RefCell, rc::Rc};
#[test]
#[ignore = "requires a connected Wi-Fi interface and WINARCHY_WIFI_TEST_DIR; read-only"]
fn wifi_provider_and_native_counters_are_read_only() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("WINARCHY_WIFI_TEST_DIR").expect("test applet folder"),
    );
    let applet = Applet {
        name: "wifi".into(),
        manifest: toml::from_str(&std::fs::read_to_string(dir.join("applet.toml")).unwrap())
            .unwrap(),
        dir,
    };
    let result = run(
        &applets::command(&applet),
        &applets::environment(&applet),
        &applet.dir,
        Some(r#"["refresh","",""]"#),
    )
    .unwrap();
    let data: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        data["error"], "",
        "read-only provider error: {}",
        data["error"]
    );
    assert_eq!(
        data["connected"], true,
        "this opt-in test requires connected Wi-Fi"
    );
    let interface = data["interface_guid"].as_str().unwrap();
    let first = traffic::read(interface).unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    let second = traffic::read(interface).unwrap();
    assert!(second.received >= first.received && second.sent >= first.sent);
    let mut tracker = crate::traffic::Tracker::default();
    tracker.update(interface, first);
    let values = tracker.update(interface, second);
    assert_ne!(values["receiving"], "—");
    assert_ne!(values["sending"], "—");
}
struct Headless(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}
fn press(window: &MinimalSoftwareWindow, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}
#[test]
#[ignore = "set WINARCHY_WIFI_TEST_DIR to a Windows-visible applet directory; optional WINARCHY_WIFI_RENDER_DIR for PPM snapshots"]
fn wifi_view_renders_and_pins_password_target() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("WINARCHY_WIFI_TEST_DIR").expect("test applet folder"),
    );
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Headless(windows.clone()))).unwrap();
    // The production shell initializes before applets. Its font registrations
    // must not force runtime text to use an ASCII-only bitmap subset.
    let _bar = shell::Bar::new().unwrap();
    let applet = Applet {
        name: "wifi".into(),
        manifest: toml::from_str(&std::fs::read_to_string(dir.join("applet.toml")).unwrap())
            .unwrap(),
        dir,
    };
    let def = compile(&applet).unwrap();
    let instance = def.create().unwrap();
    let window = windows.borrow().last().unwrap().clone();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = calls.clone();
    instance
        .set_callback("action", move |args| {
            captured.borrow_mut().push(action_argument(args).unwrap());
            Value::Void
        })
        .unwrap();
    let mut data = serde_json::json!({
        "connected":true, "ssid":"Maison | Café", "signal":92, "rate":866.0, "band":"5 GHz",
        "status":"Connected to Wi-Fi · Signal 92%", "ip":"192.168.1.207", "gateway":"192.168.1.254",
        "dns":"195.36.145.100, 195.36.228.100", "link_rate":"866 Mbps",
        "receiving":"963.1 KB/s", "sending":"4.9 KB/s", "downloaded":"1.3 GB", "uploaded":"240.0 MB", "traffic_period":"Monitoring this adapter · 42 min",
        "networks":[
            {"ssid":"Maison | Café", "signal":92, "secured":true, "known":true, "connected":true, "available":true},
            {"ssid":"Travail", "signal":78, "secured":true, "known":true, "connected":false, "available":true},
            {"ssid":"Invités | été", "signal":62, "secured":true, "known":false, "connected":false, "available":true}
        ], "error":""
    });
    set_data(&instance, &def, &data).unwrap();
    instance.show().unwrap();
    window.set_size(slint::PhysicalSize::new(480, 600));
    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    let snapshot = |name: &str| {
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            let size = window.size();
            let mut pixels = vec![Rgb8Pixel::default(); (size.width * size.height) as usize];
            renderer.render(&mut pixels, size.width as usize);
            if let Some(dir) = std::env::var_os("WINARCHY_WIFI_RENDER_DIR") {
                let mut bytes = format!("P6\n{} {}\n255\n", size.width, size.height).into_bytes();
                for pixel in pixels {
                    bytes.extend([pixel.r, pixel.g, pixel.b]);
                }
                std::fs::write(
                    std::path::PathBuf::from(dir).join(format!("{name}.ppm")),
                    bytes,
                )
                .unwrap();
            }
        });
    };
    snapshot("wifi-dark");
    press(&window, slint::platform::Key::DownArrow.into());
    press(&window, slint::platform::Key::Return.into());
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&calls.borrow()[0]).unwrap(),
        ["connect", "Travail", ""]
    );
    instance.set_property("busy", Value::Bool(true)).unwrap();
    instance.set_property("busy", Value::Bool(false)).unwrap();
    instance.invoke("completed", &[]).unwrap();
    press(&window, slint::platform::Key::DownArrow.into());
    press(&window, slint::platform::Key::Return.into());
    press(&window, "pass|word123".into());
    snapshot("wifi-password");
    data["networks"][2]["ssid"] = serde_json::json!("Different network after refresh");
    set_data(&instance, &def, &data).unwrap();
    press(&window, slint::platform::Key::Return.into());
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&calls.borrow()[1]).unwrap(),
        ["connect", "Invités | été", "pass|word123"]
    );
    instance.set_property("busy", Value::Bool(true)).unwrap();
    instance.set_property("busy", Value::Bool(false)).unwrap();
    instance.invoke("completed", &[]).unwrap();
    // Dismissal must clear the old password before another connection attempt.
    press(&window, slint::platform::Key::Return.into());
    press(&window, "must-not-survive".into());
    instance.invoke("dismissed", &[]).unwrap();
    // Reopening returns keyboard focus to the main scope (the runtime re-shows the view).
    instance.hide().unwrap();
    instance.show().unwrap();
    press(&window, slint::platform::Key::Return.into());
    press(&window, "new-password".into());
    press(&window, slint::platform::Key::Return.into());
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&calls.borrow()[2]).unwrap(),
        ["connect", "Different network after refresh", "new-password"]
    );
    instance.invoke("completed", &[]).unwrap();
    press(&window, slint::platform::Key::UpArrow.into());
    press(&window, "f".into());
    assert_eq!(calls.borrow().len(), 3, "forget must require confirmation");
    snapshot("wifi-forget");
    assert!(matches!(
        instance.invoke("cancel", &[]).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        instance.invoke("cancel", &[]).unwrap(),
        Value::Bool(false)
    ));
    assert_eq!(calls.borrow().len(), 3);
    press(&window, "f".into());
    press(&window, slint::platform::Key::Return.into());
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&calls.borrow()[3]).unwrap(),
        ["forget", "Travail", ""]
    );
    instance.invoke("completed", &[]).unwrap();
    for (prop, color) in [
        ("bg", "#ffffff"),
        ("surface", "#f7f7f7"),
        ("overlay", "#e6e6e6"),
        ("fg", "#0a0a0a"),
        ("muted", "#565656"),
        ("accent", "#0a0a0a"),
    ] {
        instance
            .set_property(
                prop,
                Value::Brush(slint::Brush::SolidColor(shell::color(color))),
            )
            .unwrap();
    }
    data["dns"] = serde_json::json!("2001:4860:4860::8888, 2606:4700:4700::1111");
    data["ssid"] = serde_json::json!("Un nom de réseau très long | 日本語 | à vérifier");
    set_data(&instance, &def, &data).unwrap();
    snapshot("wifi-light-long");
    // Compare a string made only of accented glyphs against an empty heading.
    // With the old shell bitmap registration, all these glyphs disappeared.
    let heading_pixels = |heading: &str| {
        data["ssid"] = serde_json::json!(heading);
        set_data(&instance, &def, &data).unwrap();
        window.request_redraw();
        let mut result = Vec::new();
        window.draw_if_needed(|renderer| {
            let size = window.size();
            let mut pixels = vec![Rgb8Pixel::default(); (size.width * size.height) as usize];
            renderer.render(&mut pixels, size.width as usize);
            for y in 18..45 {
                result.extend_from_slice(
                    &pixels[y * size.width as usize + 65..y * size.width as usize + 370],
                );
            }
        });
        result
    };
    let mut heading_pixels = heading_pixels;
    let empty = heading_pixels("");
    let accented = heading_pixels("éàçêÉüñ");
    assert!(!empty.is_empty());
    assert!(
        empty != accented,
        "accented glyphs disappeared after shell initialization"
    );
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.5 });
    window.set_size(slint::PhysicalSize::new(720, 900));
    snapshot("wifi-light-150");
    data["connected"] = serde_json::json!(false);
    data["networks"] = serde_json::json!([]);
    data["status"] = serde_json::json!("No Wi-Fi adapter available");
    data["error"] = serde_json::json!("Access denied: check Windows location permissions.");
    set_data(&instance, &def, &data).unwrap();
    snapshot("wifi-unavailable");
    data["error"] = serde_json::json!("");
    data["networks"] = serde_json::json!((0..64).map(|i| serde_json::json!({"ssid":format!("Réseau {i:02}"), "signal":50, "known":i<32, "connected":false, "secured":true, "available":true})).collect::<Vec<_>>());
    set_data(&instance, &def, &data).unwrap();
    for _ in 0..70 {
        press(&window, slint::platform::Key::DownArrow.into());
    }
    snapshot("wifi-long-list");
    instance.hide().unwrap();
}
