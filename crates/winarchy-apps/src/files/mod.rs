//! File manager in three columns, after yazi: parent, current directory, preview.
pub mod model;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use app::App;
#[cfg(windows)]
mod app {
    use super::{
        model::{self, Action, Files, Mode, Preview},
        win,
    };
    use crate::{
        key::Key,
        ui::{self, FileRow, FilesWindow},
    };
    use slint::{ComponentHandle, ModelRc, VecModel};
    use std::{cell::RefCell, path::PathBuf, rc::Rc};
    /// The window and its state; `resident` makes `q` hide instead of quit.
    pub struct App {
        window: FilesWindow,
        _theme: winarchy_theme::live::Subscription,
        files: Rc<RefCell<Files>>,
        render: Rc<dyn Fn(&Files)>,
    }
    fn render(window: &FilesWindow, f: &Files) {
        let row = |e: &model::Entry| FileRow {
            name: e.name.as_str().into(),
            dir: e.dir,
            hidden: e.hidden,
            selected: f.selected.contains(&e.path),
            cut: f
                .clipboard
                .as_ref()
                .is_some_and(|(paths, cut)| *cut && paths.contains(&e.path)),
            size: if e.dir {
                "".into()
            } else {
                model::size(e.size).into()
            },
        };
        let rows = |entries: &[model::Entry]| {
            ModelRc::new(VecModel::from(entries.iter().map(row).collect::<Vec<_>>()))
        };
        window.set_path(f.title().into());
        window.set_parent_rows(rows(&f.parent));
        window.set_parent_cursor(f.parent_cursor.map_or(-1, |c| c as i32));
        window.set_rows(rows(&f.entries));
        window.set_cursor(f.cursor as i32);
        let (preview_rows, lines): (&[model::Entry], Vec<&str>) = match &f.preview {
            Preview::Dir(entries) => (entries, vec![]),
            Preview::Text(lines) | Preview::Info(lines) => {
                (&[], lines.iter().map(String::as_str).collect())
            }
            Preview::Empty => (&[], vec![]),
        };
        window.set_preview_rows(rows(preview_rows));
        window.set_preview_lines(ui::strings(&lines));
        let (left, right) = f.status();
        window.set_status_left(left.into());
        window.set_status_right(right.into());
        window.set_help(f.mode == Mode::Help);
    }
    impl App {
        pub fn new(resident: bool) -> Result<Self, String> {
            let home = ui::config_home();
            let user = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default());
            let window = FilesWindow::new().map_err(|e| e.to_string())?;
            window.set_help_lines(ui::strings(&model::HELP));
            let files = Rc::new(RefCell::new(Files::open(user.clone(), user)));
            let render: Rc<dyn Fn(&Files)> = {
                let window = window.as_weak();
                Rc::new(move |f: &Files| {
                    if let Some(window) = window.upgrade() {
                        render(&window, f);
                    }
                })
            };
            {
                let files = files.clone();
                let render = render.clone();
                let weak = window.as_weak();
                window.on_key(move |text, ctrl, _shift| {
                    let Some(key) = Key::from_slint(&text) else {
                        return;
                    };
                    let Some(window) = weak.upgrade() else { return };
                    let mut f = files.borrow_mut();
                    let action = f.key(key, ctrl, window.get_page().max(1) as usize);
                    let outcome = match &action {
                        Action::None => Ok(()),
                        Action::Quit => {
                            if resident {
                                let _ = window.hide();
                            } else {
                                let _ = slint::quit_event_loop();
                            }
                            Ok(())
                        }
                        Action::Open(path) => win::open(path),
                        Action::Terminal(dir) => win::terminal(&home, dir),
                        Action::Copy { sources, into } => win::copy(sources, into, false),
                        Action::Move { sources, into } => win::copy(sources, into, true),
                        Action::Trash(paths) => win::delete(paths, true),
                        Action::Delete(paths) => win::delete(paths, false),
                        Action::Rename { from, to } => {
                            std::fs::rename(from, to).map_err(|e| e.to_string())
                        }
                        Action::CreateDir(path) => {
                            std::fs::create_dir(path).map_err(|e| e.to_string())
                        }
                        Action::CreateFile(path) => std::fs::File::create_new(path)
                            .map(|_| ())
                            .map_err(|e| e.to_string()),
                    };
                    let changes_disk = !matches!(
                        action,
                        Action::None | Action::Quit | Action::Open(_) | Action::Terminal(_)
                    );
                    if changes_disk {
                        f.reload();
                        if let Action::Rename { to, .. }
                        | Action::CreateDir(to)
                        | Action::CreateFile(to) = &action
                            && let Some(name) = to.file_name()
                        {
                            f.seek(&name.to_string_lossy());
                        }
                    }
                    if let Err(e) = outcome {
                        f.notice = e;
                    }
                    render(&f);
                });
            }
            let theme = ui::watch_theme(&window)?;
            Ok(Self {
                window,
                _theme: theme,
                files,
                render,
            })
        }
        /// Shows the window, in `dir` when given, with the theme read again so
        /// a long-lived process follows theme changes.
        pub fn show(&self, dir: Option<PathBuf>) -> Result<(), String> {
            ui::apply(
                self.window.global::<ui::Palette>(),
                &winarchy_theme::Theme::current(&ui::config_home()),
            );
            {
                let mut f = self.files.borrow_mut();
                match dir {
                    Some(dir) => f.go(dir),
                    None => f.reload(),
                }
                (self.render)(&f);
            }
            self.window.show().map_err(|e| e.to_string())?;
            // A window shown again after hide() keeps its last frame; ask for a
            // fresh one so a stale or empty surface never stays on screen.
            self.window.window().request_redraw();
            ui::raise(&self.window);
            Ok(())
        }
    }
}
#[cfg(windows)]
pub fn run() -> Result<(), String> {
    crate::ui::init_com();
    let app = App::new(false)?;
    let start = std::env::args()
        .nth(2)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir());
    app.show(start)?;
    slint::run_event_loop().map_err(|e| e.to_string())
}
