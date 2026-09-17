use super::*;
fn runtime() -> Runtime {
    let (tx, _) = crate::queue::channel(8);
    let mut runtime = Runtime::new(tx);
    runtime.generation = 2;
    runtime.entries.push(Entry {
        applet: Applet {
            name: "wifi".into(),
            dir: std::path::PathBuf::new(),
            manifest: toml::from_str("").unwrap(),
        },
        icon: None,
        icon_file: None,
        data: serde_json::json!({"ssid":"current"}),
        error: None,
        running: true,
        traffic: None,
        pending_actions: VecDeque::new(),
        due: Instant::now(),
        interval: Duration::from_secs(30),
        definition: None,
        instance: None,
    });
    runtime
}
#[test]
fn large_popups_respect_bar_and_monitor_edges() {
    let monitor = Rect {
        x: -1920,
        y: 100,
        w: 1920,
        h: 1080,
    };
    for top in [true, false] {
        let rect = popup_rect(monitor, (960, 1200), 60, 12, -10, top);
        assert_eq!(rect.h, 996);
        assert!(rect.x >= monitor.x && rect.x + rect.w <= monitor.x + monitor.w);
        assert!(rect.y >= monitor.y && rect.y + rect.h <= monitor.y + monitor.h);
        if top {
            assert_eq!(rect.y, monitor.y + 72);
        } else {
            assert_eq!(rect.y + rect.h, monitor.y + monitor.h - 72);
        }
    }
}
#[test]
fn structured_actions_preserve_delimiters_and_unicode() {
    let args = ["connect", "Café | \\\"", "password|\\\\\""];
    let values = args
        .iter()
        .map(|s| Value::String((*s).into()))
        .collect::<Vec<_>>();
    let encoded = action_argument(&values).unwrap();
    assert_eq!(serde_json::from_str::<Vec<String>>(&encoded).unwrap(), args);
    assert_eq!(action_argument(&values[..1]).unwrap(), "connect");
    assert!(action_argument(&[Value::Bool(true)]).is_none());
}
#[test]
fn busy_actions_are_bounded_and_ordered() {
    let mut runtime = runtime();
    runtime.action("wifi", None);
    assert!(runtime.entries[0].pending_actions.is_empty());
    for i in 0..12 {
        runtime.action("wifi", Some(i.to_string()));
    }
    assert_eq!(runtime.entries[0].pending_actions.len(), 8);
    assert_eq!(runtime.entries[0].pending_actions.front().unwrap(), "0");
    assert_eq!(runtime.entries[0].pending_actions.back().unwrap(), "7");
}
#[test]
fn stale_provider_results_do_not_clear_a_new_worker() {
    let mut runtime = runtime();
    runtime.apply("wifi", 1, Ok(r#"{"ssid":"old"}"#.into()));
    assert_eq!(runtime.entries[0].data["ssid"], "current");
    assert!(runtime.entries[0].running);
    runtime.apply("wifi", 2, Ok(r#"{"ssid":"new"}"#.into()));
    assert_eq!(runtime.entries[0].data["ssid"], "new");
    assert!(!runtime.entries[0].running);
}
