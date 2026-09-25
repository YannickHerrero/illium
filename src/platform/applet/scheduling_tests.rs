use super::*;
fn runtime() -> Runtime {
    let (tx, _) = crate::queue::channel(8);
    let mut runtime = Runtime::new(tx);
    runtime.generation = 2;
    runtime.entries.push(Entry {
        generation: 2,
        fingerprint: None,
        applet: Applet {
            name: "wifi".into(),
            dir: std::path::PathBuf::new(),
            manifest: toml::from_str("").unwrap(),
        },
        icon: None,
        sprite: None,
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
fn volume_polls_coalesce_and_user_slider_intent_takes_priority() {
    let mut runtime = runtime();
    runtime.entries[0].applet.manifest.provider = Some("builtin:volume".into());
    for _ in 0..20 {
        runtime.action("wifi", 2, Some("refresh".into()));
    }
    assert_eq!(runtime.entries[0].pending_actions.len(), 1);
    for action in [
        "levels",
        "refresh",
        "set 10",
        "set 20",
        "toggle-mute",
        "set 30",
    ] {
        runtime.action("wifi", 2, Some(action.into()));
    }
    assert_eq!(
        runtime.entries[0]
            .pending_actions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["set 20", "toggle-mute", "set 30"]
    );
}
#[test]
fn reload_keeps_unchanged_workers_and_invalidates_nested_source_edits() {
    let home = std::env::temp_dir().join(format!("illium-reload-applet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    Config::install(&home).unwrap();
    let dir = home.join("applets/fixture");
    std::fs::create_dir_all(dir.join("imports")).unwrap();
    std::fs::write(dir.join("applet.toml"), "icon = \"\"\ninterval = \"1h\"\n").unwrap();
    std::fs::write(dir.join("imports/shared.slint"), "old").unwrap();
    let mut config = Config::load(&home).unwrap();
    config.bar.left = vec!["fixture".into()];
    config.bar.right.clear();
    config.bar.center.clear();
    config.bar.drawer.clear();
    let (tx, _) = crate::queue::channel(8);
    let mut runtime = Runtime::new(tx);
    runtime.load(&config);
    let generation = runtime.entries[0].generation;
    let due = Instant::now() + Duration::from_secs(123);
    runtime.entries[0].due = due;
    runtime.entries[0].running = true;
    runtime.entries[0]
        .pending_actions
        .push_back("queued".into());
    runtime.load(&config);
    assert_eq!(runtime.entries[0].generation, generation);
    assert!(runtime.entries[0].running);
    assert_eq!(runtime.entries[0].due, due);
    assert_eq!(
        runtime.entries[0].pending_actions.pop_front().as_deref(),
        Some("queued")
    );
    runtime.apply("fixture", generation, Ok("{\"value\":1}".into()));
    assert_eq!(runtime.entries[0].data["value"], 1);
    std::fs::write(dir.join("imports/shared.slint"), "changed source").unwrap();
    runtime.load(&config);
    assert_ne!(runtime.entries[0].generation, generation);
    runtime.apply("fixture", generation, Ok("{\"value\":2}".into()));
    assert_eq!(runtime.entries[0].data["value"], 1);
    std::fs::remove_dir_all(home).unwrap();
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
    runtime.action("wifi", 1, Some("stale view action".into()));
    assert!(runtime.entries[0].pending_actions.is_empty());
    runtime.action("wifi", 2, None);
    assert!(runtime.entries[0].pending_actions.is_empty());
    for i in 0..12 {
        runtime.action("wifi", 2, Some(i.to_string()));
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
