//! Resident mode: the file manager and task manager windows are created once
//! and hidden, then shown on request over a named pipe, so a request pays
//! neither process start-up, Defender's scan of the executable nor the font
//! database. `q` hides the window and keeps its state.
use std::path::PathBuf;
pub const ENDPOINT_PREFIX: &str = "illium-apps";
/// `show files [dir]`, `show tasks`, `show shot`, `ping`, `quit`.
#[derive(Debug, PartialEq)]
enum Request {
    ShowFiles(Option<PathBuf>),
    ShowTasks,
    ShowShot,
    Ping,
    Quit,
}
fn parse(line: &str) -> Result<Request, String> {
    let line = line.trim();
    match line
        .split_once(' ')
        .map_or((line, ""), |(a, b)| (a, b.trim()))
    {
        ("ping", "") => Ok(Request::Ping),
        ("quit", "") => Ok(Request::Quit),
        ("show", rest) => match rest
            .split_once(' ')
            .map_or((rest, ""), |(a, b)| (a, b.trim()))
        {
            ("files", "") => Ok(Request::ShowFiles(None)),
            ("files", dir) => Ok(Request::ShowFiles(Some(PathBuf::from(dir)))),
            ("tasks", "") => Ok(Request::ShowTasks),
            ("shot", "") => Ok(Request::ShowShot),
            _ => Err(format!("unknown application: {rest}")),
        },
        _ => Err(format!("unknown request: {line}")),
    }
}
#[cfg(windows)]
pub use resident::run;
#[cfg(windows)]
mod resident {
    use super::{Request, parse};
    use std::{cell::RefCell, sync::mpsc, time::Duration};
    struct Apps {
        files: crate::files::App,
        tasks: crate::tasks::App,
        shot: crate::shot::Resident,
    }
    thread_local! {
        /// Lives on the UI thread; pipe requests reach it through the event loop.
        static APPS: RefCell<Option<Apps>> = const { RefCell::new(None) };
    }
    fn handle(request: Request) -> Result<String, String> {
        match request {
            Request::Ping => Ok("ok".into()),
            Request::Quit => {
                if APPS.with_borrow(|apps| apps.as_ref().is_some_and(|apps| apps.files.working())) {
                    return Err(
                        "a file operation is still in progress; retry quit when it finishes".into(),
                    );
                }
                slint::quit_event_loop().map_err(|e| e.to_string())?;
                Ok("ok".into())
            }
            Request::ShowFiles(dir) => APPS.with_borrow(|apps| {
                apps.as_ref()
                    .ok_or("applications not ready")?
                    .files
                    .show(dir)
                    .map(|()| "ok".into())
            }),
            Request::ShowShot => APPS.with_borrow(|apps| {
                apps.as_ref()
                    .ok_or("applications not ready")?
                    .shot
                    .show()
                    .map(|()| "ok".into())
            }),
            Request::ShowTasks => APPS.with_borrow(|apps| {
                apps.as_ref()
                    .ok_or("applications not ready")?
                    .tasks
                    .show()
                    .map(|()| "ok".into())
            }),
        }
    }
    /// Runs the request on the UI thread and waits for its outcome.
    fn dispatch(line: String) -> Result<String, String> {
        let request = parse(&line)?;
        let (tx, rx) = mpsc::channel();
        slint::invoke_from_event_loop(move || {
            let _ = tx.send(handle(request));
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| "applications did not answer in time".to_owned())?
    }
    pub fn run() -> Result<(), String> {
        crate::ui::init_com();
        let name = illium_ipc::client::pipe_path(&illium_ipc::identity::endpoint_named(
            super::ENDPOINT_PREFIX,
        )?);
        // A single instance: the pipe refuses a second one.
        let file = illium_ipc::server::create_pipe(&name)
            .map_err(|e| format!("resident applications already running: {e}"))?;
        APPS.set(Some(Apps {
            files: crate::files::App::new(true)?,
            tasks: crate::tasks::App::new(true)?,
            shot: crate::shot::Resident::new()?,
        }));
        std::thread::spawn(move || {
            illium_ipc::server::accept_loop(&file, dispatch, crate::log::write);
        });
        slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
        APPS.set(None);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests() {
        assert_eq!(parse("ping\n"), Ok(Request::Ping));
        assert_eq!(parse("show tasks"), Ok(Request::ShowTasks));
        assert_eq!(parse("show files"), Ok(Request::ShowFiles(None)));
        assert_eq!(
            parse("show files C:\\Users\\me\\my dir"),
            Ok(Request::ShowFiles(Some(PathBuf::from(
                "C:\\Users\\me\\my dir"
            ))))
        );
        assert_eq!(parse("show shot"), Ok(Request::ShowShot));
        assert!(parse("show shot extra").is_err());
        assert!(parse("open files").is_err());
        assert!(parse("").is_err());
    }
}
