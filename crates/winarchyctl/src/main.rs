fn plugin(args: &[String]) -> Result<(), String> {
    use winarchy::plugins::{self, Kind};
    let home = winarchy_theme::config_home();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let backup = match args.as_slice() {
        ["list"] => {
            for item in plugins::list(&home)? {
                println!("{:?}\t{}\t{}\t{}\t{}{}", item.kind, item.id, item.status,
                    item.origin, item.version.as_deref().unwrap_or("unknown version"),
                    item.error.map(|e| format!("\t{e}")).unwrap_or_default());
            }
            return Ok(());
        }
        ["list", "--json"] => {
            println!("{}", serde_json::to_string_pretty(&plugins::list(&home)?).map_err(|e| e.to_string())?);
            return Ok(());
        }
        ["inspect", kind, id] => {
            let kind = Kind::parse(kind)?;
            let item = plugins::list(&home)?.into_iter().find(|i| i.kind == kind && i.id == *id).ok_or("plugin not installed")?;
            println!("{}", serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?);
            return Ok(());
        }
        ["install", path] => plugins::install(&home, std::path::Path::new(path), false)?,
        ["update", path] => plugins::install(&home, std::path::Path::new(path), true)?,
        ["enable", "applet", id] => plugins::set_enabled(&home, id, true, None)?,
        ["enable", "applet", id, "--section", section] => plugins::set_enabled(&home, id, true, Some(section))?,
        ["disable", "applet", id] => plugins::set_enabled(&home, id, false, None)?,
        ["uninstall", kind, id] => plugins::uninstall(&home, Kind::parse(kind)?, id)?,
        _ => return Err("usage: winarchyctl plugin list [--json] | inspect <applet|theme> <id> | install <local-folder> | update <local-folder> | enable applet <id> [--section left|center|right|drawer] | disable applet <id> | uninstall <applet|theme> <id>\nApply themes with: winarchyctl theme set <id>".into()),
    };
    println!("done; backup: {}", backup.display());
    Ok(())
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "plugin") {
        if let Err(error) = plugin(&args[1..]) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
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
