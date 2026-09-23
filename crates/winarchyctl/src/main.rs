fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "theme") && args.get(1).is_some_and(|s| s == "install") {
        if args.len() != 3 {
            eprintln!("usage: winarchyctl theme install <pack-folder>");
            std::process::exit(2);
        }
        match winarchy_theme::pack::install(
            &winarchy_theme::config_home(),
            std::path::Path::new(&args[2]),
        ) {
            Ok(name) => println!("installed {name}; select with: winarchyctl theme set {name}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.first().is_some_and(|s| s == "lock") && args.get(1).is_some_and(|s| s == "set-password")
    {
        if args.len() != 2 {
            eprintln!("usage: winarchyctl lock set-password");
            std::process::exit(2);
        }
        match set_password() {
            Ok(()) => println!("lock password saved; lock with: winarchyctl lock"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let command = args.join(" ");
    if let Err(e) = command.parse::<winarchy_ipc::command::Command>() {
        eprintln!("{e}");
        std::process::exit(2);
    }
    #[cfg(windows)]
    match winarchy_ipc::client::client(&command) {
        Ok(reply) => {
            if reply.ok {
                println!("{}", reply.message);
            } else {
                eprintln!("{}", reply.message);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("winarchyctl requires Windows");
        std::process::exit(1);
    }
}
/// Only the Argon2 hash is stored, in the file the daemon reads at each lock
/// (`src/lockscreen.rs`).
fn set_password() -> Result<(), String> {
    use argon2::password_hash::PasswordHasher;
    let password = rpassword::prompt_password("New lock password: ").map_err(|e| e.to_string())?;
    if password.is_empty() {
        return Err("the lock password cannot be empty".into());
    }
    if rpassword::prompt_password("Repeat it: ").map_err(|e| e.to_string())? != password {
        return Err("the passwords do not match".into());
    }
    let hash = argon2::Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| e.to_string())?;
    let home = winarchy_theme::config_home();
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    std::fs::write(home.join("lock-password"), format!("{hash}\n")).map_err(|e| e.to_string())
}
