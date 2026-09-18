//! All ConPTY operations (including resize/close) run outside the UI thread.
//! Output is parsed in 16 KiB pieces, not queued without bound. Input is a
//! bounded queue; resize has a single latest-value slot. No idle polling.
use crate::{
    config::Config,
    model::{Model, Size},
    palette::Palette,
};
use alacritty_terminal::event::{Event, WindowSize};
use portable_pty::{CommandBuilder, PtySize};
use std::{
    io::{Read, Write},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, SyncSender},
    },
};

type Wake = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct Control {
    stop: bool,
    size: Option<Size>,
}
type Signal = Arc<(Mutex<Control>, Condvar)>;
fn stop(signal: &Signal) {
    signal.0.lock().unwrap().stop = true;
    signal.1.notify_one();
}
fn failed(model: &Arc<Mutex<Model>>, wake: &Wake, error: String) {
    model.lock().unwrap().error = Some(error);
    wake();
}
pub struct Session {
    pub model: Arc<Mutex<Model>>,
    pub palette: Arc<Mutex<Palette>>,
    input: SyncSender<Vec<u8>>,
    control: Signal,
}
impl Session {
    pub fn start(config: Config, size: Size, palette: Palette, wake: Wake) -> Self {
        let mut model = Model::new(size, config.scrollback);
        model.configure(config.scrollback, config.osc52_copy);
        let model = Arc::new(Mutex::new(model));
        let palette = Arc::new(Mutex::new(palette));
        let control = Arc::new((Mutex::new(Control::default()), Condvar::new()));
        let (input, rx) = mpsc::sync_channel::<Vec<u8>>(32);
        let result = Self {
            model: model.clone(),
            palette: palette.clone(),
            input: input.clone(),
            control: control.clone(),
        };
        std::thread::spawn(move || {
            let run = || -> Result<(), String> {
                if control.0.lock().unwrap().stop {
                    return Ok(());
                }
                let pair = portable_pty::native_pty_system()
                    .openpty(pty_size(size))
                    .map_err(|e| e.to_string())?;
                let mut command = CommandBuilder::new("wsl.exe");
                command.args(["--distribution", &config.distribution, "--cd", "~"]);
                command.env("TERM", "xterm-256color");
                command.env("COLORTERM", "truecolor");
                // Explicitly bridge TERM/COLORTERM into Linux without a shell wrapper.
                let mut env = std::env::var("WSLENV").unwrap_or_default();
                for key in ["TERM/u", "COLORTERM/u"] {
                    if !env.is_empty() {
                        env.push(':');
                    }
                    env.push_str(key);
                }
                command.env("WSLENV", env);
                let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
                let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
                let mut child = pair
                    .slave
                    .spawn_command(command)
                    .map_err(|e| e.to_string())?;
                drop(pair.slave);
                let mut killer = child.clone_killer();
                let exit_model = model.clone();
                let exit_wake = wake.clone();
                let exit_control = control.clone();
                std::thread::spawn(move || {
                    let _ = child.wait();
                    exit_model.lock().unwrap().exited = true;
                    stop(&exit_control);
                    exit_wake();
                });
                let write_model = model.clone();
                let write_wake = wake.clone();
                std::thread::spawn(move || {
                    while let Ok(bytes) = rx.recv() {
                        if let Err(e) = writer.write_all(&bytes) {
                            failed(&write_model, &write_wake, format!("PTY write: {e}"));
                            break;
                        }
                    }
                });
                let read_model = model.clone();
                let read_wake = wake.clone();
                std::thread::spawn(move || {
                    let mut bytes = [0; 16384];
                    loop {
                        let n = match reader.read(&mut bytes) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        let mut m = read_model.lock().unwrap();
                        let events = m.feed(&bytes[..n]);
                        use alacritty_terminal::grid::Dimensions;
                        let size = WindowSize {
                            num_lines: m.term.screen_lines() as u16,
                            num_cols: m.term.columns() as u16,
                            cell_width: 0,
                            cell_height: 0,
                        };
                        let overrides = *m.term.colors();
                        drop(m);
                        for event in events {
                            let response = match event {
                                Event::PtyWrite(s) => Some(s),
                                Event::ColorRequest(i, format) => palette
                                    .lock()
                                    .unwrap()
                                    .colors
                                    .get(i)
                                    .copied()
                                    .map(|rgb| format(overrides[i].unwrap_or(rgb))),
                                Event::TextAreaSizeRequest(format) => Some(format(size)),
                                _ => None, // Clipboard writes are queued by Model; window operations stay ignored.
                            };
                            if let Some(response) = response {
                                // Backpressure on the reader is safe; never block the UI.
                                if input.send(response.into_bytes()).is_err() {
                                    break;
                                }
                            }
                        }
                        read_wake();
                    }
                });
                let mut c = control.0.lock().unwrap();
                loop {
                    if c.stop {
                        break;
                    }
                    if let Some(size) = c.size.take() {
                        drop(c);
                        if let Err(e) = pair.master.resize(pty_size(size)) {
                            failed(&model, &wake, format!("PTY resize: {e}"));
                        }
                        c = control.0.lock().unwrap();
                    } else {
                        c = control.1.wait(c).unwrap();
                    }
                }
                drop(c);
                let _ = killer.kill();
                // ClosePseudoConsole can block while draining: this is a worker.
                drop(pair.master);
                Ok(())
            };
            if let Err(e) = run() {
                failed(&model, &wake, format!("Unable to start WSL: {e}"));
            }
        });
        result
    }
    pub fn expire_sync(&self) -> bool {
        let mut model = self.model.lock().unwrap();
        let events = model.expire_sync();
        let pending = model.sync_pending();
        let overrides = *model.term.colors();
        use alacritty_terminal::grid::Dimensions;
        let size = WindowSize {
            num_lines: model.term.screen_lines() as u16,
            num_cols: model.term.columns() as u16,
            cell_width: 0,
            cell_height: 0,
        };
        drop(model);
        for event in events {
            let response = match event {
                Event::PtyWrite(s) => Some(s),
                Event::ColorRequest(i, format) => self
                    .palette
                    .lock()
                    .unwrap()
                    .colors
                    .get(i)
                    .copied()
                    .map(|rgb| format(overrides[i].unwrap_or(rgb))),
                Event::TextAreaSizeRequest(format) => Some(format(size)),
                _ => None,
            };
            if let Some(s) = response {
                let _ = self.send(s.into_bytes());
            }
        }
        pending
    }
    pub fn send(&self, bytes: Vec<u8>) -> Result<(), &'static str> {
        if bytes.len() > 65536 {
            return Err("Input exceeds 64 KiB; paste smaller chunks");
        }
        self.input
            .try_send(bytes)
            .map_err(|_| "Terminal input queue is full or WSL is unavailable")
    }
    pub fn resize(&self, size: Size) {
        self.model.lock().unwrap().term.resize(size);
        self.control.0.lock().unwrap().size = Some(size);
        self.control.1.notify_one();
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        stop(&self.control);
    }
}
fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows as u16,
        cols: size.cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "starts a disposable Debian shell; requires WSL"]
    fn wsl_preserves_csi_u_ctrl_digits() {
        let (tx, rx) = mpsc::channel();
        let s = Session::start(
            Config::default(),
            Size::new(100, 24),
            Palette::new(&winarchy_theme::Theme::default_theme()),
            Arc::new(move || {
                let _ = tx.send(());
            }),
        );
        let command = "python3 -c 'import tty,sys,os; tty.setraw(0); print(\"READY\",flush=True); d=os.read(0,64); print(\"CSI:\"+d.hex(),flush=True)'\r";
        s.send(command.as_bytes().to_vec()).unwrap();
        let wait = |needle: &str, exact: bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                let model = s.model.lock().unwrap();
                use alacritty_terminal::{
                    grid::Dimensions,
                    index::{Column, Line, Point},
                };
                let text = model.term.bounds_to_string(
                    Point::new(Line(0), Column(0)),
                    Point::new(Line(23), Column(model.term.columns() - 1)),
                );
                if if exact {
                    text.lines().any(|line| line.trim() == needle)
                } else {
                    text.contains(needle)
                } {
                    return;
                }
                assert!(model.error.is_none(), "{:?}", model.error);
                drop(model);
                rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .expect("WSL roundtrip timed out");
            }
        };
        wait("READY", true);
        s.send(b"\x1b[49;5u".to_vec()).unwrap();
        wait("CSI:1b5b34393b3575", false);
    }
    #[test]
    #[ignore = "starts a disposable Debian Zsh; requires configured interactive shell"]
    fn wsl_prompt_is_ready_before_any_user_input() {
        use alacritty_terminal::term::TermMode;
        let (tx, rx) = mpsc::channel();
        let s = Session::start(
            Config::default(),
            Size::new(100, 24),
            Palette::new(&winarchy_theme::Theme::default_theme()),
            Arc::new(move || {
                let _ = tx.send(());
            }),
        );
        let start = std::time::Instant::now();
        loop {
            let model = s.model.lock().unwrap();
            assert!(model.error.is_none(), "{:?}", model.error);
            assert!(!model.exited, "shell exited before its prompt");
            if model.term.mode().contains(TermMode::BRACKETED_PASTE) {
                eprintln!(
                    "line editor ready without input after {:?}",
                    start.elapsed()
                );
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "line editor did not become ready without keyboard input"
            );
            drop(model);
            let _ = rx.recv_timeout(std::time::Duration::from_millis(50));
        }
        // Do not execute a command or write personal history. Verify both
        // characters at the editor's initial cursor, not just eventual output.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let point = s.model.lock().unwrap().term.grid().cursor.point;
        eprintln!("idle cursor before input: {point:?}");
        for (offset, byte) in b"ab".iter().copied().enumerate() {
            s.send(vec![byte]).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let model = s.model.lock().unwrap();
                let col = alacritty_terminal::index::Column(point.column.0 + offset);
                if model.term.grid()[point.line][col].c == char::from(byte) {
                    break;
                }
                assert!(model.error.is_none(), "{:?}", model.error);
                assert!(
                    std::time::Instant::now() < deadline,
                    "typed character {offset} was not echoed at its expected position; current cursor: {:?}",
                    model.term.grid().cursor.point,
                );
                drop(model);
                let _ = rx.recv_timeout(std::time::Duration::from_millis(50));
            }
        }
    }
    #[test]
    fn missing_distribution_reports_error_without_blocking_caller() {
        let (tx, rx) = mpsc::channel();
        let config = Config {
            distribution: "winarchy-test-no-such-distribution".into(),
            ..Config::default()
        };
        let s = Session::start(
            config,
            Size::new(80, 24),
            Palette::new(&winarchy_theme::Theme::default_theme()),
            Arc::new(move || {
                let _ = tx.send(());
            }),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            let m = s.model.lock().unwrap();
            if m.exited || m.error.is_some() {
                break;
            }
        }
    }
}
