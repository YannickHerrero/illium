use super::*;

struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new(kind: Kind, id: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        Config::install(&home).unwrap();
        let source = temp.path().join("package");
        fs::create_dir_all(source.join("payload")).unwrap();
        let f = Self {
            _temp: temp,
            home,
            source,
        };
        f.version(kind, id, "1.0.0");
        if kind == Kind::Applet {
            fs::write(
                f.source.join("payload/applet.toml"),
                "interval = '10m'\n[settings]\nstore = 'outside.json'\n",
            )
            .unwrap();
            fs::write(
                f.source.join("payload/view.slint"),
                "export component View inherits Window { }",
            )
            .unwrap();
            fs::write(
                f.source.join(format!("payload/{id}.ps1")),
                "# This provider must never run during management\nthrow 'do not execute'",
            )
            .unwrap();
        } else {
            fs::write(
                f.source.join("payload/theme.toml"),
                include_str!("../../../../config/themes/catppuccin-mocha.toml"),
            )
            .unwrap();
        }
        f
    }
    fn version(&self, kind: Kind, id: &str, version: &str) {
        let p = Package {
            schema: 1,
            id: id.into(),
            kind,
            version: version.into(),
            name: id.into(),
            description: String::new(),
        };
        fs::write(
            self.source.join("plugin.toml"),
            toml::to_string(&p).unwrap(),
        )
        .unwrap();
    }
    fn install(&self) {
        install(&self.home, &self.source, false).unwrap();
    }
}

#[test]
fn todo_lifecycle_keeps_data_placement_comments_and_local_settings() {
    let f = Fixture::new(Kind::Applet, "todo");
    let data = f._temp.path().join("tasks.json");
    fs::write(&data, "private tasks").unwrap();
    fs::write(f.home.join("bar.toml"), "# my bar\nenabled=true\nposition='top'\nheight=28\nclock_format='%H:%M'\nleft=['workspaces']\ncenter=['clock']\nright=['drawer', 'battery'] # keep\ndrawer=['wifi']\n").unwrap();
    let before = text(&f.home.join("bar.toml")).unwrap();
    f.install();
    assert_eq!(text(&f.home.join("bar.toml")).unwrap(), before);
    assert!(applets::disabled(&f.home).unwrap().contains(&"todo".into()));
    assert_eq!(
        list(&f.home)
            .unwrap()
            .into_iter()
            .find(|p| p.id == "todo")
            .unwrap()
            .version
            .as_deref(),
        Some("1.0.0")
    );
    set_enabled(&f.home, "todo", true, Some("drawer")).unwrap();
    let placed = text(&f.home.join("bar.toml")).unwrap();
    assert!(placed.contains("# my bar") && placed.contains("# keep"));
    set_enabled(&f.home, "todo", false, None).unwrap();
    assert_eq!(text(&f.home.join("bar.toml")).unwrap(), placed);
    set_enabled(&f.home, "todo", true, None).unwrap();
    assert_eq!(text(&f.home.join("bar.toml")).unwrap(), placed);
    assert_eq!(
        modules(&bar(&f.home).unwrap())
            .iter()
            .filter(|n| *n == "todo")
            .count(),
        1
    );
    assert!(
        uninstall(&f.home, Kind::Applet, "todo")
            .unwrap_err()
            .contains("disable")
    );
    set_enabled(&f.home, "todo", false, None).unwrap();
    let local_manifest = f.home.join("applets/todo/applet.toml");
    fs::write(
        &local_manifest,
        "interval='10m'\n[settings]\nstore='my-tasks.json'\n",
    )
    .unwrap();
    f.version(Kind::Applet, "todo", "1.1.0");
    fs::write(
        f.source.join("payload/applet.toml"),
        "interval='5m'\n[settings]\nstore='outside.json'\n",
    )
    .unwrap();
    let backup = install(&f.home, &f.source, true).unwrap();
    assert!(backup.join("completed.json").is_file());
    let a = applets::load(&f.home, "todo").unwrap();
    assert_eq!(a.manifest.interval, "5m");
    assert_eq!(a.manifest.settings["store"].as_str(), Some("my-tasks.json"));
    assert_eq!(text(&f.home.join("bar.toml")).unwrap(), placed);
    // A second upstream release must not erase an override merged previously.
    f.version(Kind::Applet, "todo", "1.2.0");
    install(&f.home, &f.source, true).unwrap();
    assert_eq!(
        applets::load(&f.home, "todo").unwrap().manifest.settings["store"].as_str(),
        Some("my-tasks.json")
    );
    uninstall(&f.home, Kind::Applet, "todo").unwrap();
    assert!(!f.home.join("applets/todo").exists());
    assert!(!modules(&bar(&f.home).unwrap()).contains(&"todo".into()));
    assert!(modules(&bar(&f.home).unwrap()).contains(&"wifi".into()));
    assert_eq!(text(&data).unwrap(), "private tasks");
    Config::load(&f.home).unwrap();
}

#[test]
fn attached_applet_has_no_extra_icon_and_conflicts_are_explicit() {
    let f = Fixture::new(Kind::Applet, "agenda");
    fs::write(
        f.source.join("payload/applet.toml"),
        "attach='clock'\nprovider='builtin:clock'\n",
    )
    .unwrap();
    f.install();
    assert!(
        set_enabled(&f.home, "agenda", true, None)
            .unwrap_err()
            .contains("calendar")
    );
    set_enabled(&f.home, "calendar", false, None).unwrap();
    set_enabled(&f.home, "agenda", true, None).unwrap();
    let refs = modules(&bar(&f.home).unwrap());
    assert!(!refs.contains(&"agenda".into()));
    let loaded = || {
        applets::referenced(&f.home, &[&refs])
            .into_iter()
            .flatten()
            .map(|a| a.name)
            .collect::<Vec<_>>()
    };
    assert!(loaded().contains(&"agenda".into()));
    set_enabled(&f.home, "agenda", false, None).unwrap();
    assert!(!loaded().contains(&"agenda".into()));
    assert!(!loaded().contains(&"calendar".into()));
    set_enabled(&f.home, "agenda", true, None).unwrap();
    assert!(loaded().contains(&"agenda".into()));
}

#[test]
fn theme_lifecycle_protects_active_theme_and_personal_wallpapers() {
    let f = Fixture::new(Kind::Theme, "sample");
    f.install();
    let original_theme = text(&f.home.join("winarchy.toml")).unwrap();
    assert_eq!(active_theme(&f.home).unwrap(), "catppuccin-mocha");
    fs::write(f.home.join("winarchy.toml"), "theme='sample'").unwrap();
    f.version(Kind::Theme, "sample", "2.0.0");
    assert!(
        install(&f.home, &f.source, true)
            .unwrap_err()
            .contains("another theme")
    );
    assert!(uninstall(&f.home, Kind::Theme, "sample").is_err());
    fs::write(f.home.join("winarchy.toml"), original_theme).unwrap();
    let personal = f.home.join("themes/sample/wallpapers/personal.txt");
    fs::write(&personal, "keep").unwrap();
    assert!(install(&f.home, &f.source, true).is_err());
    assert!(uninstall(&f.home, Kind::Theme, "sample").is_err());
    fs::remove_file(personal).unwrap();
    install(&f.home, &f.source, true).unwrap();
    uninstall(&f.home, Kind::Theme, "sample").unwrap();
    assert!(!f.home.join("themes/sample.toml").exists());
    assert!(!f.home.join("themes/sample").exists());
}

#[test]
fn inventory_is_read_only_and_unknown_plugins_are_never_adopted() {
    let f = Fixture::new(Kind::Applet, "devlauncher");
    fs::create_dir(f.home.join("applets/devlauncher")).unwrap();
    fs::write(
        f.home.join("applets/devlauncher/applet.toml"),
        "interval='30s'",
    )
    .unwrap();
    let rows = list(&f.home).unwrap();
    let local = rows.iter().find(|p| p.id == "devlauncher").unwrap();
    assert!(!local.managed && local.version.is_none());
    assert_eq!(local.origin, "unmanaged");
    assert!(!f.home.join(META).exists());
    assert!(!rows.iter().any(|p| p.id == "_template"));
    assert!(
        rows.iter()
            .any(|p| p.id == "weather" && p.origin == "bundled")
    );
    assert!(install(&f.home, &f.source, false).is_err());
    assert!(install(&f.home, &f.source, true).is_err());
    assert!(uninstall(&f.home, Kind::Applet, "devlauncher").is_err());
    assert!(uninstall(&f.home, Kind::Applet, "calendar").is_err());
    assert!(uninstall(&f.home, Kind::Theme, "dynamic-dark").is_err());
    assert_eq!(
        text(&f.home.join("applets/devlauncher/applet.toml")).unwrap(),
        "interval='30s'"
    );
}

#[test]
fn modified_code_and_conflicting_settings_are_not_overwritten() {
    let f = Fixture::new(Kind::Applet, "todo");
    f.install();
    f.version(Kind::Applet, "todo", "2.0.0");
    let script = f.home.join("applets/todo/todo.ps1");
    let original = text(&script).unwrap();
    fs::write(&script, "my code").unwrap();
    assert!(install(&f.home, &f.source, true).is_err());
    assert!(uninstall(&f.home, Kind::Applet, "todo").is_err());
    assert_eq!(text(&script).unwrap(), "my code");
    fs::write(script, original).unwrap();
    let manifest = f.home.join("applets/todo/applet.toml");
    fs::write(
        &manifest,
        "interval='3m'\n[settings]\nstore='outside.json'\n",
    )
    .unwrap();
    fs::write(
        f.source.join("payload/applet.toml"),
        "interval='4m'\n[settings]\nstore='outside.json'\n",
    )
    .unwrap();
    assert!(
        install(&f.home, &f.source, true)
            .unwrap_err()
            .contains("conflict")
    );
    assert!(text(&manifest).unwrap().contains("3m"));
}

#[test]
fn publication_and_validation_failures_restore_the_exact_originals() {
    let f = Fixture::new(Kind::Applet, "todo");
    let session = Session::open(&f.home).unwrap();
    let stage = session.stage().unwrap();
    let before = text(&f.home.join("bar.toml")).unwrap();
    for fail_publication in [true, false] {
        let new_bar = staged_text(stage.path(), "new-bar", "broken configuration").unwrap();
        let mut changes = vec![(PathBuf::from("bar.toml"), Some(new_bar))];
        if fail_publication {
            changes.push((
                PathBuf::from("plugins.toml"),
                Some(stage.path().join("missing")),
            ));
        }
        let error = session
            .commit(changes, || validate_config(&f.home))
            .unwrap_err();
        assert!(error.contains("rolled back"), "{error}");
        assert_eq!(text(&f.home.join("bar.toml")).unwrap(), before);
        assert!(!f.home.join("plugins.toml").exists());
    }
}

#[test]
fn lock_and_pending_journal_block_mutations() {
    let f = Fixture::new(Kind::Applet, "todo");
    let session = Session::open(&f.home).unwrap();
    assert!(Session::open(&f.home).is_err());
    drop(session);
    let pending = f.home.join(META).join("backups/interrupted");
    fs::create_dir(&pending).unwrap();
    fs::write(pending.join("pending.json"), "{}").unwrap();
    fs::hard_link(
        pending.join("pending.json"),
        f.home.join(META).join("pending.json"),
    )
    .unwrap();
    assert!(Config::load(&f.home).is_err());
    assert!(files::snapshot(&f.home).is_err());
    assert!(
        install(&f.home, &f.source, false)
            .unwrap_err()
            .contains("unfinished plugin transaction")
    );
    assert!(!f.home.join("applets/todo").exists());
}

#[test]
fn invalid_packages_and_identifiers_are_rejected_without_installing() {
    let f = Fixture::new(Kind::Applet, "todo");
    for bad in ["../escape", "CON", "nul", "a.b", "Upper", ""] {
        assert!(valid_id(bad).is_err());
    }
    fs::write(f.source.join("payload/applet.toml"), "not_a_manifest=true").unwrap();
    assert!(install(&f.home, &f.source, false).is_err());
    assert!(!f.home.join("applets/todo").exists());
    let f = Fixture::new(Kind::Theme, "sample");
    fs::write(f.source.join("payload/preview.png"), "not an image").unwrap();
    assert!(install(&f.home, &f.source, false).is_err());
    assert!(!f.home.join("themes/sample.toml").exists());
}

#[test]
fn uninstall_last_drawer_item_removes_empty_drawer_anchor() {
    let f = Fixture::new(Kind::Applet, "todo");
    f.install();
    fs::write(f.home.join("bar.toml"), "enabled=true\nposition='top'\nheight=28\nclock_format='%H:%M'\nleft=['workspaces']\ncenter=['clock']\nright=['drawer']\ndrawer=['todo']\n").unwrap();
    uninstall(&f.home, Kind::Applet, "todo").unwrap();
    Config::load(&f.home).unwrap();
    assert!(!modules(&bar(&f.home).unwrap()).contains(&"drawer".into()));
}

#[test]
fn preparation_detects_edits_before_the_backup_is_created() {
    let f = Fixture::new(Kind::Applet, "todo");
    let session = Session::open(&f.home).unwrap();
    let stage = session.stage().unwrap();
    let replacement = staged_text(
        stage.path(),
        "bar",
        &text(&f.home.join("bar.toml")).unwrap(),
    )
    .unwrap();
    fs::write(f.home.join("bar.toml"), "edited during preparation").unwrap();
    let error = session
        .commit(vec![(PathBuf::from("bar.toml"), Some(replacement))], || {
            Ok(())
        })
        .unwrap_err();
    assert!(error.contains("changed while preparing"));
    assert_eq!(
        text(&f.home.join("bar.toml")).unwrap(),
        "edited during preparation"
    );
    assert!(ensure_ready(&f.home).is_ok());
}

#[test]
fn rollback_does_not_destroy_concurrent_edits() {
    let f = Fixture::new(Kind::Applet, "todo");
    let session = Session::open(&f.home).unwrap();
    let stage = session.stage().unwrap();
    let replacement = staged_text(
        stage.path(),
        "bar",
        &text(&f.home.join("bar.toml")).unwrap(),
    )
    .unwrap();
    let error = session
        .commit(vec![(PathBuf::from("bar.toml"), Some(replacement))], || {
            fs::write(f.home.join("bar.toml"), "concurrent personal edits").unwrap();
            Err("injected failure".into())
        })
        .unwrap_err();
    assert!(error.contains("rollback failed"));
    assert_eq!(
        text(&f.home.join("bar.toml")).unwrap(),
        "concurrent personal edits"
    );
    assert!(ensure_ready(&f.home).is_err());
}

#[test]
fn malformed_applet_can_be_disabled_and_is_reported() {
    let f = Fixture::new(Kind::Applet, "todo");
    f.install();
    fs::write(f.home.join("applets/todo/applet.toml"), "unknown=true").unwrap();
    assert_eq!(
        list(&f.home)
            .unwrap()
            .into_iter()
            .find(|i| i.id == "todo")
            .unwrap()
            .status,
        "error"
    );
    set_enabled(&f.home, "todo", false, None).unwrap();
    assert!(set_enabled(&f.home, "todo", true, None).is_err());
}

#[test]
#[ignore = "requires WINARCHY_TODO_SOURCE and WINARCHY_THEME_SOURCE; only copies into temporary homes"]
fn local_collection_packages() {
    for (kind, id, variable) in [
        (Kind::Applet, "todo", "WINARCHY_TODO_SOURCE"),
        (Kind::Theme, "sample", "WINARCHY_THEME_SOURCE"),
    ] {
        let source =
            PathBuf::from(std::env::var(variable).expect("set source environment variable"));
        let f = Fixture::new(kind, id);
        remove(&f.source.join("payload")).unwrap();
        if kind == Kind::Applet {
            fs::create_dir(f.source.join("payload")).unwrap();
            for file in [
                "applet.toml",
                "view.slint",
                "todo.ps1",
                "icon.svg",
                "check.svg",
                "plus.svg",
                "trash.svg",
                "README.md",
            ] {
                copy(&source.join(file), &f.source.join("payload").join(file)).unwrap();
            }
        } else {
            copy(&source, &f.source.join("payload")).unwrap();
        }
        f.install();
        if kind == Kind::Applet {
            set_enabled(&f.home, id, true, None).unwrap();
            set_enabled(&f.home, id, false, None).unwrap();
            set_enabled(&f.home, id, true, None).unwrap();
            set_enabled(&f.home, id, false, None).unwrap();
        }
        f.version(kind, id, "1.1.0");
        install(&f.home, &f.source, true).unwrap();
        uninstall(&f.home, kind, id).unwrap();
        Config::load(&f.home).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn links_in_payload_destination_and_metadata_are_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new(Kind::Applet, "todo");
    symlink(f._temp.path(), f.source.join("payload/link")).unwrap();
    assert!(install(&f.home, &f.source, false).is_err());
    fs::remove_file(f.source.join("payload/link")).unwrap();
    symlink(f._temp.path(), f.home.join("applets/todo")).unwrap();
    assert!(install(&f.home, &f.source, false).is_err());
    fs::remove_file(f.home.join("applets/todo")).unwrap();
    fs::remove_dir_all(f.home.join(META)).unwrap();
    symlink(f._temp.path(), f.home.join(META)).unwrap();
    assert!(install(&f.home, &f.source, false).is_err());
}
