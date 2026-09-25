use super::{Event, EventSender};
use crate::request::{ReplyTo, Ticket};
pub use illium_ipc::client::{client, pipe_name};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn dispatch(tx: &EventSender, command: crate::command::Command) -> Result<String, String> {
    let (sender, rx) = std::sync::mpsc::channel();
    let ticket = Arc::new(Ticket::new(Instant::now() + Duration::from_secs(5)));
    tx.send(Event::Command(
        command,
        Some(ReplyTo {
            ticket: ticket.clone(),
            sender,
        }),
    ))
    .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result,
        Err(_) if ticket.cancel() => Err("IPC request cancelled before execution".into()),
        Err(_) => {
            Err("IPC execution already started; outcome unknown, do not retry blindly".into())
        }
    }
}
pub fn start(tx: EventSender) -> Result<(), String> {
    let file = illium_ipc::server::create_pipe(&pipe_name()?)?;
    std::thread::spawn(move || {
        illium_ipc::server::accept_loop(
            &file,
            |line| line.parse().and_then(|command| dispatch(&tx, command)),
            |e| tracing::debug!(e, "IPC"),
        );
    });
    Ok(())
}
