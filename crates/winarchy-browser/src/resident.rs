//! One prepared window per Windows user/session. Extra simultaneous windows are
//! standalone; we do not keep an unbounded pool of Chromium instances alive.
use crate::native;
use std::{sync::mpsc, time::Duration};
use winarchy_ipc::{client, identity, server};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{AllowSetForegroundWindow, PostThreadMessageW},
};

pub const REQUEST: u32 = 0x8007;
pub struct Request {
    pub command: String,
    pub reply: mpsc::SyncSender<Result<String, String>>,
}
#[derive(PartialEq)]
pub enum Exit {
    Closed,
    Quit,
}
fn request(pipe: &str, command: &str) -> Result<String, String> {
    let reply = client::client_at(pipe, command, Duration::from_secs(8))?;
    if reply.ok {
        Ok(reply.message)
    } else {
        Err(reply.message)
    }
}
pub fn log(message: &str) {
    use std::io::Write;
    eprintln!("{message}");
    let home = winarchy_theme::config_home();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.join("browser.log"))
    {
        let _ = writeln!(file, "{message}");
    }
}
pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--demo"] {
        // Dedicated process: never contact the resident or open the normal profile.
        let data = winarchy_browser::demo::DemoData::new().map_err(|e| e.to_string())?;
        return native::run_demo(data.path())
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    if args.first().is_some_and(|a| a == "--standalone") {
        return native::run(&args[1..].join(" "), false, None, |_| {})
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    let serve = args.as_slice() == ["--serve"];
    let control = args.as_slice() == ["--status"] || args.as_slice() == ["--quit"];
    if args.first().is_some_and(|a| a.starts_with("--")) && !serve && !control {
        return Err(
            "Usage: winarchy-browser [URL | --serve | --standalone [URL] | --demo | --status | --quit]"
                .into(),
        );
    }
    let pipe = client::pipe_path(&identity::endpoint_named("winarchy-browser")?);
    if control {
        let answer = request(&pipe, args[0].trim_start_matches("--"))?;
        println!("{answer}");
        return Ok(());
    }
    let file = match server::create_pipe(&pipe) {
        Ok(file) => file,
        Err(_) => {
            let pid = request(&pipe, "pid")?
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            if serve {
                return Ok(());
            }
            unsafe {
                let _ = AllowSetForegroundWindow(pid);
            }
            // An ambiguous reply must never cause another open/fallback launch.
            request(
                &pipe,
                &format!(
                    "open {}",
                    serde_json::to_string(&args.join(" ")).map_err(|e| e.to_string())?
                ),
            )?;
            return Ok(());
        }
    };
    let (tx, rx) = mpsc::sync_channel(4);
    let mut server = Some((file, tx));
    let mut hidden = serve;
    let mut target = if serve { String::new() } else { args.join(" ") };
    loop {
        let result = native::run(&target, hidden, Some(&rx), |thread| {
            // Retain one owner-only pipe and one server thread across window rebuilds.
            if let Some((file, tx)) = server.take() {
                std::thread::spawn(move || {
                    server::accept_loop(
                        &file,
                        |command| {
                            if command == "pid" {
                                return Ok(std::process::id().to_string());
                            }
                            let (reply, answer) = mpsc::sync_channel(1);
                            tx.try_send(Request { command, reply })
                                .map_err(|_| "Browser UI is busy".to_string())?;
                            unsafe { PostThreadMessageW(thread, REQUEST, WPARAM(0), LPARAM(0)) }
                                .map_err(|e| e.to_string())?;
                            answer.recv_timeout(Duration::from_secs(3)).map_err(|_| {
                                "Browser request timed out; outcome unknown".to_string()
                            })?
                        },
                        log,
                    )
                });
            }
        })
        .map_err(|e| e.to_string())?;
        if result == Exit::Quit {
            return Ok(());
        }
        // Closing releases the old controller/page (including audio and unload
        // handlers), then prepares a fresh hidden window. No page stays loaded.
        target.clear();
        hidden = true;
    }
}
