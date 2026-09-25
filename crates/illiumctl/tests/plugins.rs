use std::{
    fs,
    process::{Command, Output},
};

#[test]
fn cli_local_package_lifecycle_and_json_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    illium_config::config::Config::install(&home).unwrap();
    let source = temp.path().join("package");
    fs::create_dir_all(source.join("payload")).unwrap();
    let manifest = "schema=1\nid='sample'\nkind='theme'\nversion='1.0.0'\nname='Sample'\n";
    fs::write(source.join("plugin.toml"), manifest).unwrap();
    fs::write(
        source.join("payload/theme.toml"),
        include_str!("../../../config/themes/catppuccin-mocha.toml"),
    )
    .unwrap();
    let run = |args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_illiumctl"))
            .env("ILLIUM_CONFIG_HOME", &home)
            .args(args)
            .output()
            .unwrap()
    };
    let success = |args: &[&str]| -> Output {
        let output = run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    success(&["plugin", "list", "--json"]);
    assert!(!home.join(".plugins").exists());
    success(&["plugin", "install", source.to_str().unwrap()]);
    let result = success(&["plugin", "inspect", "theme", "sample"]);
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["version"], "1.0.0");
    assert_eq!(json["managed"], true);
    fs::write(
        source.join("plugin.toml"),
        manifest.replace("1.0.0", "1.1.0"),
    )
    .unwrap();
    success(&["plugin", "update", source.to_str().unwrap()]);
    success(&["plugin", "uninstall", "theme", "sample"]);
    assert!(!home.join("themes/sample.toml").exists());
    assert!(
        !run(&["plugin", "uninstall", "theme", "catppuccin-mocha"])
            .status
            .success()
    );
    assert!(!run(&["plugin", "install"]).status.success());
}
