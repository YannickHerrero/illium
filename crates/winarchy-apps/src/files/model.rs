//! File manager state: three columns (parent, current, preview), selection,
//! clipboard and prompts. Directories are read with std::fs; every operation
//! that changes the disk is returned as an `Action` for the caller to run.
use crate::key::{Key, matches, step};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    time::SystemTime,
};
/// Directories with more entries than this are cut, with a notice.
pub const MAX_ENTRIES: usize = 10_000;
const PREVIEW_ENTRIES: usize = 200;
const PREVIEW_BYTES: usize = 16 * 1024;
const PREVIEW_LINES: usize = 200;
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    pub hidden: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Natural,
    Size,
    Modified,
}
impl Sort {
    fn next(self) -> Self {
        match self {
            Self::Natural => Self::Size,
            Self::Size => Self::Modified,
            Self::Modified => Self::Natural,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Natural => "name",
            Self::Size => "size",
            Self::Modified => "modified",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    Rename(PathBuf),
    Create,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Selection extends from the anchor to the cursor.
    Visual(usize),
    Filter,
    Prompt(Prompt, String),
    ConfirmTrash(Vec<PathBuf>),
    ConfirmDelete(Vec<PathBuf>),
    Help,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    Empty,
    Dir(Vec<Entry>),
    Text(Vec<String>),
    Info(Vec<String>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    /// Open with the default application.
    Open(PathBuf),
    /// Open the configured terminal in the directory.
    Terminal(PathBuf),
    Copy {
        sources: Vec<PathBuf>,
        into: PathBuf,
    },
    Move {
        sources: Vec<PathBuf>,
        into: PathBuf,
    },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    CreateDir(PathBuf),
    CreateFile(PathBuf),
}
pub const HELP: [&str; 15] = [
    "h / l, Enter       parent / enter or open",
    "j / k, gg / G      move, first / last",
    "Space, v, Ctrl+a   select, visual mode, select all",
    "y / x / p          yank, cut, paste here",
    "d / D              trash / delete permanently (asks y/n)",
    "r                  rename",
    "a                  create (trailing / makes a directory)",
    ".                  toggle hidden files",
    "/                  filter, Enter keeps it, Escape clears",
    "s                  cycle sort: name, size, modified",
    "o                  terminal here",
    "~                  home directory",
    "w                  home in the default WSL distribution",
    "?                  this help",
    "q                  quit",
];
/// Read-only jobs contain no UI state and can safely run on one worker.
pub enum ReadRequest {
    Directory { revision: u64, cwd: Option<PathBuf> },
    Preview { revision: u64, entry: Option<Entry>, hidden: bool, sort: Sort },
}
pub enum ReadResult {
    Directory { revision: u64, entries: Vec<Entry>, parent: Vec<Entry>, notice: String },
    Preview { revision: u64, preview: Preview },
}
impl ReadRequest {
    pub fn run(self) -> ReadResult {
        match self {
            Self::Directory { revision, cwd } => {
                let (entries, notice) = match &cwd {
                    Some(dir) => match read_dir(dir) {
                        Ok((entries, notice)) => (entries, notice.unwrap_or_default()),
                        Err(error) => (vec![], error),
                    },
                    None => (root_entries(), String::new()),
                };
                let parent = match cwd.as_ref() {
                    None => vec![],
                    Some(dir) => match dir.parent() {
                        Some(parent) => read_dir(parent).map(|(entries, _)| entries).unwrap_or_default(),
                        None => root_entries(),
                    },
                };
                ReadResult::Directory { revision, entries, parent, notice }
            }
            Self::Preview { revision, entry, hidden, sort } => {
                let preview = match entry {
                    None => Preview::Empty,
                    Some(e) if e.dir => match read_dir(&e.path) {
                        Ok((mut entries, _)) => {
                            arrange_entries(&mut entries, "", hidden, sort);
                            entries.truncate(PREVIEW_ENTRIES);
                            Preview::Dir(entries)
                        }
                        Err(error) => Preview::Info(vec![error]),
                    },
                    Some(e) => preview_file(&e),
                };
                ReadResult::Preview { revision, preview }
            }
        }
    }
}
pub struct Files {
    /// Current directory; `None` lists the drives.
    pub cwd: Option<PathBuf>,
    /// Visible entries of the current directory, sorted and filtered.
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub parent: Vec<Entry>,
    pub parent_cursor: Option<usize>,
    pub preview: Preview,
    pub selected: BTreeSet<PathBuf>,
    /// Paths yanked or cut, and whether they are cut.
    pub clipboard: Option<(Vec<PathBuf>, bool)>,
    pub show_hidden: bool,
    pub filter: String,
    pub sort: Sort,
    pub mode: Mode,
    pub notice: String,
    pending_g: bool,
    /// Name under the cursor per directory, so coming back lands on it.
    memory: HashMap<PathBuf, String>,
    all: Vec<Entry>,
    home: PathBuf,
    deferred: bool,
    directory_revision: u64,
    preview_revision: u64,
    directory_pending: bool,
    preview_pending: bool,
    pub loading: bool,
}
fn arrange_entries(entries: &mut Vec<Entry>, filter: &str, hidden: bool, sort: Sort) {
    entries.retain(|e| (hidden || !e.hidden) && matches(filter, &e.name));
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| match sort {
        Sort::Natural => natural(&a.name, &b.name),
        Sort::Size => b.size.cmp(&a.size).then_with(|| natural(&a.name, &b.name)),
        Sort::Modified => b.modified.cmp(&a.modified).then_with(|| natural(&a.name, &b.name)),
    }));
}
/// Case-insensitive comparison treating digit runs as numbers: `a2` < `a10`.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().filter(|c| c.is_ascii_digit()) {
                    n.push(*c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().filter(|d| d.is_ascii_digit()) {
                    m.push(*d);
                    y.next();
                }
                let order = n
                    .trim_start_matches('0')
                    .len()
                    .cmp(&m.trim_start_matches('0').len())
                    .then_with(|| n.trim_start_matches('0').cmp(m.trim_start_matches('0')));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c
                    .to_lowercase()
                    .cmp(d.to_lowercase())
                    .then_with(|| c.cmp(&d));
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata, name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x2 != 0 || name.starts_with('.')
}
#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata, name: &str) -> bool {
    name.starts_with('.')
}
/// Every entry of `dir`, unsorted; a notice when the listing was cut.
pub fn read_dir(dir: &Path) -> Result<(Vec<Entry>, Option<String>), String> {
    let mut entries = Vec::new();
    let mut cut = None;
    for item in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let Ok(item) = item else { continue };
        if entries.len() >= MAX_ENTRIES {
            cut = Some(format!("showing the first {MAX_ENTRIES} entries"));
            break;
        }
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(meta) = item.metadata() else { continue };
        let dir = meta.is_dir()
            || (meta.file_type().is_symlink()
                && std::fs::metadata(&path).is_ok_and(|m| m.is_dir()));
        entries.push(Entry {
            hidden: is_hidden(&meta, &name),
            size: if dir { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            name,
            path,
            dir,
        });
    }
    Ok((entries, cut))
}
/// Top of the tree: the drives, then the WSL distributions as `wsl: <name>`.
#[cfg(windows)]
pub fn roots() -> Vec<(String, PathBuf)> {
    super::win::roots()
}
#[cfg(not(windows))]
pub fn roots() -> Vec<(String, PathBuf)> {
    vec![("/".into(), PathBuf::from("/"))]
}
/// Home directory in the default WSL distribution, when one is registered.
#[cfg(windows)]
fn wsl_root() -> Option<PathBuf> {
    super::win::default_wsl_home()
}
#[cfg(not(windows))]
fn wsl_root() -> Option<PathBuf> {
    None
}
fn root_entries() -> Vec<Entry> {
    roots()
        .into_iter()
        .map(|(name, path)| Entry {
            name,
            path,
            dir: true,
            hidden: false,
            size: 0,
            modified: None,
        })
        .collect()
}
impl Files {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn open(start: PathBuf, home: PathBuf) -> Self {
        Self::new(start, home, false)
    }
    /// Construct without touching disk; the UI schedules `take_read()` jobs.
    pub fn deferred(start: PathBuf, home: PathBuf) -> Self {
        Self::new(start, home, true)
    }
    fn new(start: PathBuf, home: PathBuf, deferred: bool) -> Self {
        let mut f = Self {
            cwd: None,
            entries: vec![],
            cursor: 0,
            parent: vec![],
            parent_cursor: None,
            preview: Preview::Empty,
            selected: BTreeSet::new(),
            clipboard: None,
            show_hidden: false,
            filter: String::new(),
            sort: Sort::Natural,
            mode: Mode::Normal,
            notice: String::new(),
            pending_g: false,
            memory: HashMap::new(),
            all: vec![],
            home,
            deferred,
            directory_revision: 0,
            preview_revision: 0,
            directory_pending: false,
            preview_pending: false,
            loading: false,
        };
        f.enter(Some(start));
        f
    }
    /// Changes directory and restores the cursor remembered for it.
    fn enter(&mut self, dir: Option<PathBuf>) {
        if let (Some(cwd), Some(e)) = (&self.cwd, self.entries.get(self.cursor)) {
            self.memory.insert(cwd.clone(), e.name.clone());
        }
        if self.deferred && self.cwd != dir {
            // Never make the previous directory's rows actionable under a new path.
            self.entries.clear();
            self.all.clear();
            self.parent.clear();
            self.parent_cursor = None;
            self.preview = Preview::Empty;
            self.cursor = 0;
        }
        self.cwd = dir;
        self.filter.clear();
        self.selected.clear();
        self.mode = Mode::Normal;
        self.reload();
        let remembered = self
            .cwd
            .as_ref()
            .and_then(|d| self.memory.get(d))
            .and_then(|name| self.entries.iter().position(|e| e.name == *name));
        self.cursor = remembered.unwrap_or(0);
        self.refresh_preview();
    }
    /// Changes to `dir`, as the caller's request rather than a key.
    pub fn go(&mut self, dir: PathBuf) {
        self.enter(Some(dir));
    }
    /// Re-reads the current directory, keeping the cursor on the same name.
    pub fn reload(&mut self) {
        if self.deferred {
            self.directory_revision = self.directory_revision.wrapping_add(1);
            self.preview_revision = self.preview_revision.wrapping_add(1);
            self.directory_pending = true;
            self.preview_pending = false;
            self.loading = true;
            return;
        }
        let name = self.entries.get(self.cursor).map(|e| e.name.clone());
        self.notice.clear();
        match &self.cwd {
            None => {
                self.all = root_entries();
                self.parent = vec![];
                self.parent_cursor = None;
            }
            Some(dir) => {
                match read_dir(dir) {
                    Ok((entries, cut)) => {
                        self.all = entries;
                        if let Some(cut) = cut {
                            self.notice = cut;
                        }
                    }
                    Err(e) => {
                        self.all = vec![];
                        self.notice = e;
                    }
                }
                let (parent, cursor) = match dir.parent() {
                    Some(p) => {
                        let mut entries = read_dir(p).map(|(e, _)| e).unwrap_or_default();
                        self.arrange(&mut entries, "");
                        let cursor = entries.iter().position(|e| e.path == *dir);
                        (entries, cursor)
                    }
                    None => {
                        let entries = root_entries();
                        let cursor = entries.iter().position(|e| e.path == *dir);
                        (entries, cursor)
                    }
                };
                self.parent = parent;
                self.parent_cursor = cursor;
            }
        }
        self.apply_view();
        self.cursor = name
            .and_then(|n| self.entries.iter().position(|e| e.name == n))
            .unwrap_or(self.cursor.min(self.entries.len().saturating_sub(1)));
        let existing: BTreeSet<PathBuf> = self.all.iter().map(|e| e.path.clone()).collect();
        self.selected.retain(|p| existing.contains(p));
        self.refresh_preview();
    }
    fn arrange(&self, entries: &mut Vec<Entry>, filter: &str) {
        arrange_entries(entries, filter, self.show_hidden, self.sort);
    }
    pub fn take_read(&mut self) -> Option<ReadRequest> {
        if std::mem::take(&mut self.directory_pending) {
            Some(ReadRequest::Directory { revision: self.directory_revision, cwd: self.cwd.clone() })
        } else if !self.loading && std::mem::take(&mut self.preview_pending) {
            Some(ReadRequest::Preview {
                revision: self.preview_revision, entry: self.current().cloned(),
                hidden: self.show_hidden, sort: self.sort,
            })
        } else { None }
    }
    /// Apply only data, never a worker's copy of selection, filter or clipboard.
    pub fn apply_read(&mut self, result: ReadResult) -> bool {
        match result {
            ReadResult::Directory { revision, entries, mut parent, notice } => {
                if revision != self.directory_revision { return false; }
                let name = self.current().map(|e| e.name.clone()).or_else(|| {
                    self.cwd.as_ref().and_then(|cwd| self.memory.get(cwd).cloned())
                });
                self.all = entries;
                self.arrange(&mut parent, "");
                self.parent_cursor = self.cwd.as_ref().and_then(|cwd| parent.iter().position(|e| &e.path == cwd));
                self.parent = parent;
                self.notice = notice;
                self.loading = false;
                self.apply_view();
                self.cursor = name.and_then(|name| self.entries.iter().position(|e| e.name == name))
                    .unwrap_or(self.cursor.min(self.entries.len().saturating_sub(1)));
                let existing: BTreeSet<_> = self.all.iter().map(|e| &e.path).collect();
                self.selected.retain(|p| existing.contains(p));
                self.refresh_preview();
            }
            ReadResult::Preview { revision, preview } => {
                if revision != self.preview_revision { return false; }
                self.preview = preview;
            }
        }
        true
    }
    fn apply_view(&mut self) {
        let mut entries = self.all.clone();
        self.arrange(&mut entries, &self.filter);
        self.entries = entries;
    }
    fn refresh_preview(&mut self) {
        if self.deferred {
            self.preview_revision = self.preview_revision.wrapping_add(1);
            self.preview_pending = true;
            self.preview = Preview::Empty;
            return;
        }
        self.preview = match self.entries.get(self.cursor) {
            None => Preview::Empty,
            Some(e) if e.dir => match read_dir(&e.path) {
                Ok((mut entries, _)) => {
                    self.arrange(&mut entries, "");
                    entries.truncate(PREVIEW_ENTRIES);
                    Preview::Dir(entries)
                }
                Err(err) => Preview::Info(vec![err]),
            },
            Some(e) => preview_file(e),
        };
    }
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }
    /// Paths an operation applies to: the selection, else the entry under the cursor.
    pub fn targets(&self) -> Vec<PathBuf> {
        if self.selected.is_empty() {
            self.current().map(|e| e.path.clone()).into_iter().collect()
        } else {
            self.selected.iter().cloned().collect()
        }
    }
    fn place(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.entries.len().saturating_sub(1));
        if let Mode::Visual(anchor) = self.mode {
            let (lo, hi) = (anchor.min(self.cursor), anchor.max(self.cursor));
            self.selected = self.entries[lo..=hi.min(self.entries.len().saturating_sub(1))]
                .iter()
                .map(|e| e.path.clone())
                .collect();
        }
        self.refresh_preview();
    }
    pub fn key(&mut self, key: Key, ctrl: bool, page: usize) -> Action {
        self.notice.clear();
        match self.mode.clone() {
            Mode::Filter => {
                match key {
                    Key::Escape => {
                        self.filter.clear();
                        self.mode = Mode::Normal;
                    }
                    Key::Enter => self.mode = Mode::Normal,
                    Key::Backspace => {
                        self.filter.pop();
                    }
                    Key::Char(c) if !ctrl => self.filter.push(c),
                    _ => return self.move_cursor(key, ctrl, page),
                }
                self.apply_view();
                self.place(0);
                Action::None
            }
            Mode::Prompt(kind, mut text) => {
                match key {
                    Key::Escape => self.mode = Mode::Normal,
                    Key::Enter => {
                        self.mode = Mode::Normal;
                        return self.submit(kind, text);
                    }
                    Key::Backspace => {
                        text.pop();
                        self.mode = Mode::Prompt(kind, text);
                    }
                    Key::Char('u') if ctrl => self.mode = Mode::Prompt(kind, String::new()),
                    Key::Char(c) if !ctrl => {
                        text.push(c);
                        self.mode = Mode::Prompt(kind, text);
                    }
                    _ => {}
                }
                Action::None
            }
            Mode::ConfirmTrash(paths) => {
                self.mode = Mode::Normal;
                if key == Key::Char('y') {
                    self.selected.clear();
                    Action::Trash(paths)
                } else {
                    Action::None
                }
            }
            Mode::ConfirmDelete(paths) => {
                self.mode = Mode::Normal;
                if key == Key::Char('y') {
                    self.selected.clear();
                    Action::Delete(paths)
                } else {
                    Action::None
                }
            }
            Mode::Help => {
                self.mode = Mode::Normal;
                Action::None
            }
            Mode::Normal | Mode::Visual(_) => self.normal(key, ctrl, page),
        }
    }
    fn normal(&mut self, key: Key, ctrl: bool, page: usize) -> Action {
        if key == Key::Char('g') && !ctrl {
            if std::mem::take(&mut self.pending_g) {
                self.place(0);
            } else {
                self.pending_g = true;
            }
            return Action::None;
        }
        if std::mem::take(&mut self.pending_g) {
            return Action::None;
        }
        let visual = matches!(self.mode, Mode::Visual(_));
        match key {
            Key::Char('a') if ctrl => {
                self.selected = self.entries.iter().map(|e| e.path.clone()).collect();
                Action::None
            }
            Key::Char('d') if ctrl => self.move_cursor(Key::PageDown, false, page / 2),
            Key::Char('u') if ctrl => self.move_cursor(Key::PageUp, false, page / 2),
            Key::Escape if visual || !self.selected.is_empty() => {
                self.mode = Mode::Normal;
                self.selected.clear();
                Action::None
            }
            Key::Char('q') | Key::Escape => Action::Quit,
            Key::Char('h') | Key::Left | Key::Backspace => {
                let up = match &self.cwd {
                    Some(dir) => dir.parent().map(Path::to_path_buf),
                    None => return Action::None,
                };
                self.enter(up);
                Action::None
            }
            Key::Char('l') | Key::Right | Key::Enter => match self.current().cloned() {
                Some(e) if e.dir => {
                    self.enter(Some(e.path));
                    Action::None
                }
                Some(e) => Action::Open(e.path),
                None => Action::None,
            },
            Key::Char(' ') => {
                if let Some(e) = self.current() {
                    let path = e.path.clone();
                    if !self.selected.remove(&path) {
                        self.selected.insert(path);
                    }
                }
                self.move_cursor(Key::Down, false, page)
            }
            Key::Char('v') => {
                self.mode = if visual {
                    Mode::Normal
                } else {
                    Mode::Visual(self.cursor)
                };
                if !visual {
                    self.place(self.cursor);
                }
                Action::None
            }
            Key::Char('y') | Key::Char('x') => {
                let paths = self.targets();
                if !paths.is_empty() {
                    let cut = key == Key::Char('x');
                    self.notice = format!("{} {}", paths.len(), if cut { "cut" } else { "yanked" });
                    self.clipboard = Some((paths, cut));
                    self.selected.clear();
                    self.mode = Mode::Normal;
                }
                Action::None
            }
            Key::Char('p') => match (self.clipboard.take(), &self.cwd) {
                (Some((sources, cut)), Some(into)) => {
                    let into = into.clone();
                    if cut {
                        Action::Move { sources, into }
                    } else {
                        // Yanked paths stay available for another paste.
                        self.clipboard = Some((sources.clone(), false));
                        Action::Copy { sources, into }
                    }
                }
                (clipboard, _) => {
                    self.clipboard = clipboard;
                    Action::None
                }
            },
            Key::Char('d') | Key::Char('D') => {
                let paths = self.targets();
                if !paths.is_empty() && self.cwd.is_some() {
                    self.mode = if key == Key::Char('d') {
                        Mode::ConfirmTrash(paths)
                    } else {
                        Mode::ConfirmDelete(paths)
                    };
                }
                Action::None
            }
            Key::Char('r') => {
                if let Some(e) = self.current() {
                    self.mode = Mode::Prompt(Prompt::Rename(e.path.clone()), e.name.clone());
                }
                Action::None
            }
            Key::Char('a') => {
                if self.cwd.is_some() {
                    self.mode = Mode::Prompt(Prompt::Create, String::new());
                }
                Action::None
            }
            Key::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.reload();
                Action::None
            }
            Key::Char('/') => {
                self.mode = Mode::Filter;
                Action::None
            }
            Key::Char('s') => {
                self.sort = self.sort.next();
                self.reload();
                Action::None
            }
            Key::Char('o') => self.cwd.clone().map_or(Action::None, Action::Terminal),
            Key::Char('~') => {
                self.enter(Some(self.home.clone()));
                Action::None
            }
            Key::Char('w') => {
                match wsl_root() {
                    Some(root) => self.enter(Some(root)),
                    None => self.notice = "no WSL distribution registered".into(),
                }
                Action::None
            }
            Key::Char('?') => {
                self.mode = Mode::Help;
                Action::None
            }
            _ => self.move_cursor(key, ctrl, page),
        }
    }
    fn move_cursor(&mut self, key: Key, ctrl: bool, page: usize) -> Action {
        if !ctrl && let Some(c) = step(self.cursor, self.entries.len(), key, page) {
            self.place(c);
        }
        Action::None
    }
    fn submit(&mut self, kind: Prompt, text: String) -> Action {
        let text = text.trim();
        let Some(cwd) = self.cwd.clone() else {
            return Action::None;
        };
        if text.is_empty() {
            return Action::None;
        }
        match kind {
            Prompt::Rename(from) => {
                if text.contains(['/', '\\']) {
                    self.notice = "a name cannot contain a path separator".into();
                    return Action::None;
                }
                let to = cwd.join(text);
                if to == from {
                    return Action::None;
                }
                self.memory.insert(cwd, text.to_owned());
                Action::Rename { from, to }
            }
            Prompt::Create => {
                let dir = text.ends_with(['/', '\\']);
                let path = cwd.join(text.trim_end_matches(['/', '\\']));
                self.memory.insert(
                    cwd,
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                if dir {
                    Action::CreateDir(path)
                } else {
                    Action::CreateFile(path)
                }
            }
        }
    }
    /// Moves the cursor to the entry named `name` if it exists.
    pub fn seek(&mut self, name: &str) {
        if let Some(i) = self.entries.iter().position(|e| e.name == name) {
            self.place(i);
        }
    }
    pub fn title(&self) -> String {
        match &self.cwd {
            Some(d) => d.to_string_lossy().into_owned(),
            None => "Drives".into(),
        }
    }
    pub fn status(&self) -> (String, String) {
        let left = match &self.mode {
            Mode::Filter => format!("/{}", self.filter),
            Mode::Prompt(Prompt::Rename(_), text) => format!("rename: {text}"),
            Mode::Prompt(Prompt::Create, text) => format!("create: {text}"),
            Mode::ConfirmTrash(paths) => format!("move {} to the recycle bin? y/n", count(paths)),
            Mode::ConfirmDelete(paths) => format!("delete {} permanently? y/n", count(paths)),
            Mode::Help => "? help".into(),
            Mode::Visual(_) => format!("VISUAL  {} selected", self.selected.len()),
            Mode::Normal if !self.notice.is_empty() => self.notice.clone(),
            Mode::Normal if !self.filter.is_empty() => {
                format!("/{}  Escape clears", self.filter)
            }
            Mode::Normal if !self.selected.is_empty() => {
                format!("{} selected  Escape clears", self.selected.len())
            }
            Mode::Normal => "? help".into(),
        };
        let mut right = format!("{} items", self.entries.len());
        if let Some((paths, cut)) = &self.clipboard {
            right = format!(
                "{} {}  {right}",
                paths.len(),
                if *cut { "cut" } else { "yanked" }
            );
        }
        if self.show_hidden {
            right.push_str("  hidden");
        }
        if self.sort != Sort::Natural {
            right.push_str(&format!("  sort: {}", self.sort.label()));
        }
        (left, right)
    }
}
fn count(paths: &[PathBuf]) -> String {
    match paths {
        [one] => one.file_name().map_or_else(
            || one.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        ),
        many => format!("{} items", many.len()),
    }
}
fn preview_file(e: &Entry) -> Preview {
    let mut info = vec![size(e.size)];
    if let Some(ext) = e.path.extension() {
        info.push(ext.to_string_lossy().to_lowercase());
    }
    if let Some(t) = e.modified
        && let Ok(age) = SystemTime::now().duration_since(t)
    {
        info.push(format!("modified {} ago", ago(age.as_secs())));
    }
    let Ok(mut file) = std::fs::File::open(&e.path) else {
        return Preview::Info(info);
    };
    let mut buffer = vec![0u8; PREVIEW_BYTES];
    let mut read = 0;
    while read < buffer.len() {
        match std::io::Read::read(&mut file, &mut buffer[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => return Preview::Info(info),
        }
    }
    buffer.truncate(read);
    if buffer.is_empty() || buffer.contains(&0) {
        return Preview::Info(info);
    }
    let text = String::from_utf8_lossy(&buffer);
    Preview::Text(
        text.lines()
            .take(PREVIEW_LINES)
            .map(|l| l.chars().take(200).collect())
            .collect(),
    )
}
fn ago(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tree() -> Tmp {
        let p = std::env::temp_dir().join(format!(
            "winarchy-files-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("docs/notes")).unwrap();
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("file10.txt"), "ten\nlines").unwrap();
        std::fs::write(p.join("file2.txt"), "two").unwrap();
        std::fs::write(p.join(".hidden"), "").unwrap();
        std::fs::write(p.join("docs/notes/a.md"), "# a").unwrap();
        std::fs::write(p.join("blob.bin"), [0u8, 1, 2]).unwrap();
        Tmp(p)
    }
    fn names(f: &Files) -> Vec<&str> {
        f.entries.iter().map(|e| e.name.as_str()).collect()
    }
    fn finish_reads(f: &mut Files) {
        while let Some(job) = f.take_read() { assert!(f.apply_read(job.run())); }
    }
    #[test]
    fn deferred_open_and_latest_directory_win() {
        let t = tree();
        let mut f = Files::deferred(t.0.clone(), t.0.clone());
        assert!(f.loading && f.entries.is_empty());
        let stale = f.take_read().unwrap();
        f.go(t.0.join("docs"));
        assert!(!f.apply_read(stale.run()));
        assert!(f.loading && f.entries.is_empty());
        finish_reads(&mut f);
        assert!(!f.loading);
        assert_eq!(names(&f), ["notes"]);
    }
    #[test]
    fn refresh_preserves_live_selection_filter_and_clipboard() {
        let t = tree();
        let mut f = Files::deferred(t.0.clone(), t.0.clone());
        finish_reads(&mut f);
        f.reload();
        let job = f.take_read().unwrap();
        assert!(!f.entries.is_empty()); // stale-while-revalidate
        f.seek("file2.txt");
        f.selected.insert(t.0.join("file2.txt"));
        f.clipboard = Some((vec![t.0.join("file10.txt")], false));
        f.filter = "file".into();
        assert!(f.apply_read(job.run()));
        assert_eq!(names(&f), ["file2.txt", "file10.txt"]);
        assert_eq!(f.current().unwrap().name, "file2.txt");
        assert!(f.selected.contains(&t.0.join("file2.txt")));
        assert!(f.clipboard.is_some());
    }
    #[test]
    fn stale_preview_cannot_replace_current_cursor() {
        let t = tree();
        let mut f = Files::deferred(t.0.clone(), t.0.clone());
        finish_reads(&mut f);
        f.seek("file2.txt");
        let old = f.take_read().unwrap();
        f.seek("file10.txt");
        assert!(!f.apply_read(old.run()));
        finish_reads(&mut f);
        assert_eq!(f.preview, Preview::Text(vec!["ten".into(), "lines".into()]));
    }
    #[test]
    fn natural_order() {
        assert_eq!(natural("a2", "a10"), Ordering::Less);
        assert_eq!(natural("B", "a"), Ordering::Greater);
        assert_eq!(natural("x", "x"), Ordering::Equal);
        assert_eq!(natural("file 007", "file 7"), Ordering::Less);
        assert_eq!(natural("file 7", "file 007"), Ordering::Greater);
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
    }
    #[test]
    fn lists_dirs_first_hidden_off_and_previews() {
        let t = tree();
        let mut f = Files::open(t.0.clone(), t.0.clone());
        assert_eq!(
            names(&f),
            ["docs", "src", "blob.bin", "file2.txt", "file10.txt"]
        );
        assert!(matches!(f.preview, Preview::Dir(ref e) if e.len() == 1));
        f.key(Key::Char('.'), false, 10);
        assert_eq!(names(&f)[2], ".hidden");
        f.key(Key::Char('G'), false, 10);
        assert_eq!(f.preview, Preview::Text(vec!["ten".into(), "lines".into()]));
        f.seek("blob.bin");
        assert!(matches!(f.preview, Preview::Info(ref i) if i[0] == "3 B" && i[1] == "bin"));
    }
    #[test]
    fn navigation_remembers_cursor() {
        let t = tree();
        let mut f = Files::open(t.0.clone(), t.0.clone());
        f.key(Key::Char('j'), false, 10);
        assert_eq!(f.current().unwrap().name, "src");
        assert_eq!(f.key(Key::Char('l'), false, 10), Action::None);
        assert_eq!(f.cwd.as_deref(), Some(t.0.join("src").as_path()));
        assert!(f.entries.is_empty());
        assert_eq!(f.parent_cursor, Some(1));
        f.key(Key::Char('h'), false, 10);
        assert_eq!(f.current().unwrap().name, "src");
        f.key(Key::Char('~'), false, 10);
        assert_eq!(f.cwd.as_deref(), Some(t.0.as_path()));
        f.seek("file2.txt");
        assert_eq!(
            f.key(Key::Enter, false, 10),
            Action::Open(t.0.join("file2.txt"))
        );
        assert_eq!(
            f.key(Key::Char('o'), false, 10),
            Action::Terminal(t.0.clone())
        );
    }
    #[test]
    fn selection_clipboard_and_paste() {
        let t = tree();
        let mut f = Files::open(t.0.clone(), t.0.clone());
        f.key(Key::Char(' '), false, 10);
        f.key(Key::Char(' '), false, 10);
        assert_eq!(f.selected.len(), 2);
        assert_eq!(f.cursor, 2);
        f.key(Key::Escape, false, 10);
        assert!(f.selected.is_empty());
        f.key(Key::Char('v'), false, 10);
        f.key(Key::Char('j'), false, 10);
        assert_eq!(f.selected.len(), 2);
        assert!(f.status().0.starts_with("VISUAL"));
        f.key(Key::Char('y'), false, 10);
        assert_eq!(f.mode, Mode::Normal);
        assert!(f.selected.is_empty());
        f.key(Key::Char('h'), false, 10);
        let into = f.cwd.clone().unwrap();
        assert_eq!(
            f.key(Key::Char('p'), false, 10),
            Action::Copy {
                sources: vec![t.0.join("blob.bin"), t.0.join("file2.txt")],
                into: into.clone()
            }
        );
        assert!(f.clipboard.is_some());
        f.key(Key::Char('l'), false, 10);
        f.key(Key::Char('l'), false, 10);
        f.key(Key::Char('x'), false, 10);
        f.key(Key::Char('h'), false, 10);
        assert!(matches!(
            f.key(Key::Char('p'), false, 10),
            Action::Move { .. }
        ));
        assert!(f.clipboard.is_none());
        assert_eq!(f.key(Key::Char('p'), false, 10), Action::None);
    }
    #[test]
    fn trash_rename_create_and_filter() {
        let t = tree();
        let mut f = Files::open(t.0.clone(), t.0.clone());
        f.key(Key::Char('/'), false, 10);
        for c in "file".chars() {
            f.key(Key::Char(c), false, 10);
        }
        assert_eq!(names(&f), ["file2.txt", "file10.txt"]);
        f.key(Key::Enter, false, 10);
        f.key(Key::Char('d'), false, 10);
        assert!(f.status().0.contains("file2.txt"));
        assert_eq!(f.key(Key::Char('n'), false, 10), Action::None);
        f.key(Key::Char('D'), false, 10);
        assert_eq!(
            f.key(Key::Char('y'), false, 10),
            Action::Delete(vec![t.0.join("file2.txt")])
        );
        f.key(Key::Char('r'), false, 10);
        assert_eq!(f.status().0, "rename: file2.txt");
        f.key(Key::Backspace, false, 10);
        f.key(Key::Char('c'), false, 10);
        assert_eq!(
            f.key(Key::Enter, false, 10),
            Action::Rename {
                from: t.0.join("file2.txt"),
                to: t.0.join("file2.txc")
            }
        );
        f.key(Key::Char('a'), false, 10);
        for c in "new/".chars() {
            f.key(Key::Char(c), false, 10);
        }
        assert_eq!(
            f.key(Key::Enter, false, 10),
            Action::CreateDir(t.0.join("new"))
        );
        f.key(Key::Char('a'), false, 10);
        f.key(Key::Char('n'), false, 10);
        assert_eq!(
            f.key(Key::Enter, false, 10),
            Action::CreateFile(t.0.join("n"))
        );
        f.key(Key::Char('/'), false, 10);
        f.key(Key::Escape, false, 10);
        assert_eq!(f.entries.len(), 5);
        assert_eq!(f.key(Key::Char('q'), false, 10), Action::Quit);
    }
    #[test]
    fn drives_view_above_root() {
        let t = tree();
        let mut f = Files::open(t.0.clone(), t.0.clone());
        while f.cwd.is_some() {
            f.key(Key::Char('h'), false, 10);
        }
        assert_eq!(f.title(), "Drives");
        assert!(!f.entries.is_empty());
        assert_eq!(f.key(Key::Char('h'), false, 10), Action::None);
        f.key(Key::Char('l'), false, 10);
        assert!(f.cwd.is_some());
        assert_eq!(f.parent_cursor, Some(0));
        if cfg!(not(windows)) {
            f.key(Key::Char('w'), false, 10);
            assert_eq!(f.status().0, "no WSL distribution registered");
        }
    }
}
