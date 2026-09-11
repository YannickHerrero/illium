#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args().collect();
        let home = winarchy::config::Config::home();
        let _ = std::fs::create_dir_all(&home);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(home.join("winarchy.log"));
        if let Ok(file) = file {
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_max_level(if args.iter().any(|a| a == "--debug") {
                    tracing::Level::DEBUG
                } else {
                    tracing::Level::INFO
                })
                .with_writer(std::sync::Mutex::new(file))
                .init();
        }
        if let Some(i) = args.iter().position(|a| a == "--watch-session") {
            if let Some(pid) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                let _ = winarchy::platform::watchdog(pid);
            }
            return;
        }
        if let Err(e) = winarchy::platform::run(args.iter().any(|a| a == "--replace-explorer")) {
            tracing::error!(%e,"Winarchy stopped");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("Winarchy requires Windows 11 x64");
        std::process::exit(1);
    }
}
