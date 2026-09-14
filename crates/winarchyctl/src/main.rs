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
