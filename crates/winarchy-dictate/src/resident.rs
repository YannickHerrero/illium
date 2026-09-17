//! One resident per Windows user and session, elected by the pipe: the UI
//! thread owns the indicator, a controller thread owns the microphone and the
//! model, and the pipe thread only forwards the daemon's verbs.
use crate::indicator::{self, State};
use std::{sync::mpsc, time::Duration};
use winarchy_ipc::{client, identity, server};
const PREFIX: &str = "winarchy-dictate";
/// Shorter presses are accidental; the model would only invent words for them.
const MIN_SECONDS: f32 = 0.3;
enum Request {
    Start,
    Stop,
    Quit,
}
fn request(pipe: &str, command: &str) -> Result<String, String> {
    let reply = client::client_at(pipe, command, Duration::from_secs(3))?;
    if reply.ok {
        Ok(reply.message)
    } else {
        Err(reply.message)
    }
}
pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pipe = client::pipe_path(&identity::endpoint_named(PREFIX)?);
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => {}
        [verb @ ("--quit" | "--status" | "--start" | "--stop")] => {
            let answer = request(
                &pipe,
                verb.trim_start_matches("--")
                    .replace("status", "ping")
                    .as_str(),
            )?;
            crate::log(&format!("{verb}: {answer}"));
            return Ok(());
        }
        _ => return Err("Usage: winarchy-dictate [serve|--status|--start|--stop|--quit]".into()),
    }
    // FIRST_PIPE_INSTANCE elects a single resident even during simultaneous launches.
    let Ok(file) = server::create_pipe(&pipe) else {
        return Ok(());
    };
    let ui = indicator::create()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || controller(rx, ui));
    let requests = tx.clone();
    std::thread::spawn(move || {
        server::accept_loop(
            &file,
            |line| {
                let verb = match line.trim() {
                    "ping" => return Ok("ok".into()),
                    "start" => Request::Start,
                    "stop" => Request::Stop,
                    "quit" => Request::Quit,
                    other => return Err(format!("unknown dictation request: {other}")),
                };
                requests
                    .send(verb)
                    .map_err(|_| "dictation controller stopped".to_owned())?;
                Ok("ok".into())
            },
            crate::log,
        );
    });
    indicator::run_loop();
    let _ = tx.send(Request::Quit);
    Ok(())
}
fn controller(rx: mpsc::Receiver<Request>, ui: indicator::Handle) {
    let mut model = None;
    let mut capture: Option<crate::audio::Capture> = None;
    // Prewarm: the first press should find the model in memory.
    match prepare(&ui, true) {
        Ok(loaded) => model = Some(loaded),
        Err(e) => crate::log(&format!("model not ready at startup: {e}")),
    }
    while let Ok(request) = rx.recv() {
        match request {
            Request::Quit => break,
            Request::Start => {
                if capture.is_some() {
                    continue;
                }
                if model.is_none() {
                    match prepare(&ui, false) {
                        Ok(loaded) => model = Some(loaded),
                        Err(e) => {
                            crate::log(&e);
                            ui.set(State::Notice(e));
                            continue;
                        }
                    }
                }
                let level_ui = ui;
                match crate::audio::Capture::start(move |level| {
                    level_ui.set(State::Listening { level });
                }) {
                    Ok(started) => {
                        capture = Some(started);
                        ui.set(State::Listening { level: 0.0 });
                    }
                    Err(e) => {
                        crate::log(&e);
                        ui.set(State::Notice(e));
                    }
                }
            }
            Request::Stop => {
                let Some(recording) = capture.take() else {
                    continue;
                };
                let samples = recording.finish();
                if (samples.len() as f32) < MIN_SECONDS * crate::audio::SAMPLE_RATE as f32 {
                    ui.set(State::Hidden);
                    continue;
                }
                ui.set(State::Transcribing);
                let Some(loaded) = model.as_mut() else {
                    ui.set(State::Notice("model not loaded".into()));
                    continue;
                };
                let started = std::time::Instant::now();
                match crate::model::transcribe(loaded, &samples) {
                    Ok(text) if text.is_empty() => ui.set(State::Notice("Nothing heard".into())),
                    Ok(text) => {
                        crate::log(&format!(
                            "transcribed {:.1} s of audio in {} ms",
                            samples.len() as f32 / crate::audio::SAMPLE_RATE as f32,
                            started.elapsed().as_millis()
                        ));
                        match crate::paste::paste(&text) {
                            Ok(()) => ui.set(State::Hidden),
                            Err(e) => {
                                crate::log(&e);
                                ui.set(State::Notice(e));
                            }
                        }
                    }
                    Err(e) => {
                        crate::log(&e);
                        ui.set(State::Notice(e));
                    }
                }
            }
        }
    }
    drop(capture);
    ui.quit();
}
/// Fetches the model if needed and loads it; the indicator only shows the
/// wait when a download is involved or a press is waiting on it.
fn prepare(
    ui: &indicator::Handle,
    quiet: bool,
) -> Result<transcribe_rs::onnx::parakeet::ParakeetModel, String> {
    let dir = match crate::model::model_dir() {
        Ok(dir) if dir.join("vocab.txt").is_file() => dir,
        _ => {
            ui.set(State::Downloading);
            crate::model::ensure().inspect_err(|_| ui.set(State::Hidden))?
        }
    };
    if !quiet {
        ui.set(State::Loading);
    }
    let started = std::time::Instant::now();
    let model = crate::model::load(&dir).inspect_err(|_| ui.set(State::Hidden))?;
    crate::log(&format!(
        "model loaded in {} ms",
        started.elapsed().as_millis()
    ));
    if !quiet {
        ui.set(State::Hidden);
    }
    Ok(model)
}
