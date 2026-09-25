#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args().collect();
        // Reject elevation before opening user-controlled configuration/log paths.
        if let Err(e) = illium::platform::require_standard_user() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        let home = illium::config::Config::home();
        let _ = std::fs::create_dir_all(&home);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(home.join("illium.log"));
        if let Ok(file) = file {
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_max_level(if args.iter().any(|a| a == "--trace-latency") {
                    tracing::Level::TRACE
                } else if args.iter().any(|a| a == "--debug") {
                    tracing::Level::DEBUG
                } else {
                    tracing::Level::INFO
                })
                .with_writer(std::sync::Mutex::new(file))
                .init();
        }
        if let Some(i) = args.iter().position(|a| a == "--watch-session") {
            let result = match (
                args.get(i + 1).and_then(|s| s.parse().ok()),
                args.get(i + 2),
                args.get(i + 3).and_then(|s| s.parse().ok()),
            ) {
                (Some(pid), Some(identity), Some(started)) => {
                    illium::platform::watchdog(pid, identity, started)
                }
                _ => Err("invalid recovery helper arguments".into()),
            };
            if let Err(e) = result {
                tracing::error!(%e,"recovery helper stopped");
                std::process::exit(1);
            }
            return;
        }
        if let Err(e) = illium::platform::run(args.iter().any(|a| a == "--replace-explorer")) {
            tracing::error!(%e,"Illium stopped");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("Illium requires Windows 11 x64");
        std::process::exit(1);
    }
}
