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
    use slint::{ComponentHandle, Model, ModelRc, VecModel};
    use std::{cell::{Cell, RefCell}, path::PathBuf, rc::Rc, sync::mpsc};
    /// At most one read is in flight. Further input only replaces the model's
    /// pending intention, so rapid cursor movement cannot spawn a thread storm.
    struct Reader {
        busy: Cell<bool>,
        files: Rc<RefCell<Files>>,
        window: slint::Weak<FilesWindow>,
        tx: mpsc::SyncSender<model::ReadResult>,
    }
    impl Reader {
        fn schedule(&self) {
            if self.busy.get() { return; }
            let Some(job) = self.files.borrow_mut().take_read() else { return; };
            self.busy.set(true);
            let tx = self.tx.clone();
            let window = self.window.clone();
            std::thread::spawn(move || {
                let result = job.run();
                if tx.send(result).is_ok() {
                    let _ = window.upgrade_in_event_loop(|window| window.invoke_read_ready());
                }
            });
        }
    }
    /// The window and its state; `resident` makes `q` hide instead of quit.
    pub struct App {
        window: FilesWindow,
        _theme: winarchy_theme::live::Subscription,
        files: Rc<RefCell<Files>>,
        render: Rc<dyn Fn(&Files)>,
        reader: Rc<Reader>,
    }
    struct Rows {
        parent: Rc<VecModel<FileRow>>,
        current: Rc<VecModel<FileRow>>,
        preview: Rc<VecModel<FileRow>>,
        lines: Rc<VecModel<slint::SharedString>>,
    }
    impl Rows {
        fn new(window: &FilesWindow) -> Self {
            let rows = Self {
                parent: Rc::new(VecModel::default()), current: Rc::new(VecModel::default()),
                preview: Rc::new(VecModel::default()), lines: Rc::new(VecModel::default()),
            };
            window.set_parent_rows(ModelRc::from(rows.parent.clone()));
            window.set_rows(ModelRc::from(rows.current.clone()));
            window.set_preview_rows(ModelRc::from(rows.preview.clone()));
            window.set_preview_lines(ModelRc::from(rows.lines.clone()));
            rows
        }
    }
    fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: impl IntoIterator<Item = T>) {
        let mut len = 0;
        for (i, row) in rows.into_iter().enumerate() {
            match model.row_data(i) {
                Some(current) if current == row => {},
                Some(_) => model.set_row_data(i, row),
                None => model.push(row),
            }
            len += 1;
        }
        while model.row_count() > len { model.remove(model.row_count() - 1); }
    }
    #[test]
    fn differential_models_grow_change_and_shrink() {
        let model = VecModel::from(vec![1, 2]);
        sync(&model, [1, 3, 4]);
        assert_eq!(model.iter().collect::<Vec<_>>(), vec![1, 3, 4]);
        sync(&model, [1, 3, 4]);
        sync(&model, [5]);
        assert_eq!(model.iter().collect::<Vec<_>>(), vec![5]);
        sync(&model, []);
        assert_eq!(model.row_count(), 0);
    }
    fn render(window: &FilesWindow, models: &Rows, f: &Files) {
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
        window.set_path(f.title().into());
        sync(&models.parent, f.parent.iter().map(row));
        window.set_parent_cursor(f.parent_cursor.map_or(-1, |c| c as i32));
        sync(&models.current, f.entries.iter().map(row));
        window.set_cursor(f.cursor as i32);
        let (preview_rows, lines): (&[model::Entry], Vec<&str>) = match &f.preview {
            Preview::Dir(entries) => (entries, vec![]),
            Preview::Text(lines) | Preview::Info(lines) => {
                (&[], lines.iter().map(String::as_str).collect())
            }
            Preview::Empty => (&[], vec![]),
        };
        sync(&models.preview, preview_rows.iter().map(row));
        sync(&models.lines, lines.into_iter().map(slint::SharedString::from));
        let (left, right) = f.status();
        window.set_loading(f.loading);
        window.set_status_left(if f.loading { "Reading…".into() } else { left.into() });
        window.set_status_right(right.into());
        window.set_help(f.mode == Mode::Help);
    }
    impl App {
        pub fn new(resident: bool) -> Result<Self, String> {
            let home = ui::config_home();
            let user = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default());
            let window = FilesWindow::new().map_err(|e| e.to_string())?;
            window.set_help_lines(ui::strings(&model::HELP));
            let files = Rc::new(RefCell::new(Files::deferred(user.clone(), user)));
            let render: Rc<dyn Fn(&Files)> = {
                let models = Rows::new(&window);
                let window = window.as_weak();
                Rc::new(move |f: &Files| {
                    if let Some(window) = window.upgrade() {
                        render(&window, &models, f);
                    }
                })
            };
            let (tx, rx) = mpsc::sync_channel(1);
            let reader = Rc::new(Reader {
                busy: Cell::new(false), files: files.clone(), window: window.as_weak(), tx,
            });
            {
                let reader = reader.clone();
                let render = render.clone();
                window.on_read_ready(move || {
                    if let Ok(result) = rx.try_recv() {
                        reader.busy.set(false);
                        let mut f = reader.files.borrow_mut();
                        if f.apply_read(result) { render(&f); }
                        drop(f);
                        reader.schedule();
                    }
                });
            }
            {
                let files = files.clone();
                let reader = reader.clone();
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
                    drop(f);
                    reader.schedule();
                });
            }
            let theme = ui::watch_theme(&window)?;
            Ok(Self {
                window,
                _theme: theme,
                files,
                render,
                reader,
            })
        }
        /// Shows the window, in `dir` when given, with the theme read again so
        /// a long-lived process follows theme changes.
        pub fn show(&self, dir: Option<PathBuf>) -> Result<(), String> {
            // Theme subscription keeps the resident current; no disk read on show.
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
            self.reader.schedule();
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
