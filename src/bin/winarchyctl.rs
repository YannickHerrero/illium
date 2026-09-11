fn main() {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if let Err(e) = command.parse::<winarchy::command::Command>() {
        eprintln!("{e}");
        std::process::exit(2);
    }
    #[cfg(windows)]
    match winarchy::platform::ipc::client(&command) {
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
