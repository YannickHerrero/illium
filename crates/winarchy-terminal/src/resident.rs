//! One resident per Windows user/session, using Winarchy's owner-only pipe.
//! A second launch is only a client: it grants foreground permission and asks
//! the existing UI thread for a NEW window. No hidden shell is prestarted.
use crate::native::{self, REQUEST, Request};
use std::{sync::mpsc, time::Duration};
use winarchy_ipc::{client, identity, server};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{AllowSetForegroundWindow, PostThreadMessageW},
};
fn request(pipe: &str, command: &str) -> Result<String, String> {
    let reply = client::client_at(pipe, command, Duration::from_secs(8))?;
    if reply.ok {
        Ok(reply.message)
    } else {
        Err(reply.message)
    }
}
pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = match args.as_slice() {
        [] => "open",
        [arg] => match arg.as_str() {
            "--serve" => "serve",
            "--standalone" => "standalone",
            "--quit" => "quit",
            "--status" => "status",
            _ => {
                return Err(
                    "Usage: winarchy-terminal [--serve|--standalone|--status|--quit]".into(),
                );
            }
        },
        _ => return Err("Usage: winarchy-terminal [--serve|--standalone|--status|--quit]".into()),
    };
    if command == "standalone" {
        return native::run(false, true, None, |_| {});
    }
    let pipe = client::pipe_path(&identity::endpoint_named("winarchy-terminal")?);
    if matches!(command, "quit" | "status") {
        let answer = request(&pipe, command)?;
        println!("{answer}");
        crate::log(&answer);
        return Ok(());
    }
    // FIRST_PIPE_INSTANCE atomically elects the server even during simultaneous
    // cold launches. Never retry an 'open' with an unknown outcome.
    let file = match server::create_pipe(&pipe) {
        Ok(file) => file,
        Err(_) => {
            let pid = request(&pipe, "pid")?
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            if command == "serve" {
                return Ok(());
            }
            unsafe {
                let _ = AllowSetForegroundWindow(pid);
            }
            request(&pipe, "open")?;
            return Ok(());
        }
    };
    let (tx, rx) = mpsc::sync_channel(4);
    native::run(true, command == "open", Some(rx), move |thread| {
        std::thread::spawn(move || {
            server::accept_loop(
                &file,
                |command| {
                    let (reply, answer) = mpsc::sync_channel(1);
                    tx.try_send(Request { command, reply })
                        .map_err(|_| "Terminal UI is busy".to_string())?;
                    unsafe { PostThreadMessageW(thread, REQUEST, WPARAM(0), LPARAM(0)) }
                        .map_err(|e| e.to_string())?;
                    answer
                        .recv_timeout(Duration::from_secs(3))
                        .map_err(|_| "Terminal request timed out; outcome unknown".to_string())?
                },
                crate::log,
            )
        });
    })
}
