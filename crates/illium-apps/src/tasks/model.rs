//! Process list, sorting, filtering and key handling without any Win32 call.
use crate::key::{Key, matches, step};
#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    /// Percent of all logical processors over the last sampling interval.
    pub cpu: f32,
    /// Working set in bytes.
    pub memory: u64,
    /// Whether the process could be opened at all; protected ones cannot be ended.
    pub accessible: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Cpu,
    Memory,
    Name,
    Pid,
}
impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Cpu => Self::Memory,
            Self::Memory => Self::Name,
            Self::Name => Self::Pid,
            Self::Pid => Self::Cpu,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Name => "name",
            Self::Pid => "pid",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Typing into the filter.
    Filter,
    /// Waiting for y/n before ending the process.
    Confirm(u32),
    Help,
}
/// What the caller must do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Kill(u32),
}
pub struct Tasks {
    processes: Vec<Process>,
    pub sort: Sort,
    pub filter: String,
    pub cursor: usize,
    pub mode: Mode,
    /// Pid under the cursor, kept across refreshes so the cursor follows it.
    pinned: Option<u32>,
    /// A `g` was typed and waits for the second one.
    pending_g: bool,
    pub notice: String,
}
pub const HELP: [&str; 8] = [
    "j / k, arrows      move",
    "PageUp / PageDown  page",
    "gg / G             first / last",
    "/                  filter, Enter keeps it, Escape clears",
    "s                  cycle sort: cpu, memory, name, pid",
    "x                  end process (asks y/n)",
    "?                  this help",
    "q / Escape         quit",
];
impl Default for Tasks {
    fn default() -> Self {
        Self {
            processes: vec![],
            sort: Sort::Cpu,
            filter: String::new(),
            cursor: 0,
            mode: Mode::Normal,
            pinned: None,
            pending_g: false,
            notice: String::new(),
        }
    }
}
impl Tasks {
    /// Replaces the sample; the cursor stays on the same process when it survives.
    pub fn update(&mut self, processes: Vec<Process>) {
        self.processes = processes;
        self.settle();
    }
    fn settle(&mut self) {
        let pids: Vec<u32> = self.rows().iter().map(|p| p.pid).collect();
        self.cursor = self
            .pinned
            .and_then(|pid| pids.iter().position(|&p| p == pid))
            .unwrap_or_else(|| self.cursor.min(pids.len().saturating_sub(1)));
        self.pinned = pids.get(self.cursor).copied();
    }
    /// Filtered and sorted processes, ties broken by pid for a stable order.
    pub fn rows(&self) -> Vec<&Process> {
        let mut rows: Vec<&Process> = self
            .processes
            .iter()
            .filter(|p| matches(&self.filter, &p.name))
            .collect();
        rows.sort_by(|a, b| {
            match self.sort {
                Sort::Cpu => b.cpu.total_cmp(&a.cpu),
                Sort::Memory => b.memory.cmp(&a.memory),
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::Pid => a.pid.cmp(&b.pid),
            }
            .then(a.pid.cmp(&b.pid))
        });
        rows
    }
    pub fn selected(&self) -> Option<&Process> {
        self.rows().get(self.cursor).copied()
    }
    pub fn key(&mut self, key: Key, page: usize) -> Action {
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
                    Key::Char(c) => self.filter.push(c),
                    _ => return self.move_cursor(key, page),
                }
                self.place(0);
                Action::None
            }
            Mode::Confirm(pid) => {
                self.mode = Mode::Normal;
                if key == Key::Char('y') {
                    Action::Kill(pid)
                } else {
                    Action::None
                }
            }
            Mode::Help => {
                self.mode = Mode::Normal;
                Action::None
            }
            Mode::Normal => match key {
                Key::Char('g') => {
                    // `gg` goes to the top; a lone g waits for the second one.
                    if self.pending_g {
                        self.pending_g = false;
                        self.place(0);
                    } else {
                        self.pending_g = true;
                    }
                    Action::None
                }
                _ if std::mem::take(&mut self.pending_g) => Action::None,
                Key::Char('q') | Key::Escape => Action::Quit,
                Key::Char('/') => {
                    self.mode = Mode::Filter;
                    Action::None
                }
                Key::Char('s') => {
                    self.sort = self.sort.next();
                    self.settle();
                    Action::None
                }
                Key::Char('?') => {
                    self.mode = Mode::Help;
                    Action::None
                }
                Key::Char('x') => {
                    match self.selected() {
                        Some(p) if p.accessible => self.mode = Mode::Confirm(p.pid),
                        Some(p) => self.notice = format!("{} cannot be ended", p.name),
                        None => {}
                    }
                    Action::None
                }
                _ => self.move_cursor(key, page),
            },
        }
    }
    fn move_cursor(&mut self, key: Key, page: usize) -> Action {
        if let Some(c) = step(self.cursor, self.rows().len(), key, page) {
            self.place(c);
        }
        Action::None
    }
    fn place(&mut self, cursor: usize) {
        self.cursor = cursor;
        self.pinned = self.rows().get(cursor).map(|p| p.pid);
    }
    /// One line for the status bar: mode, filter, sort and count.
    pub fn status(&self) -> (String, String) {
        let left = match &self.mode {
            Mode::Filter => format!("/{}", self.filter),
            Mode::Confirm(pid) => {
                let name = self
                    .processes
                    .iter()
                    .find(|p| p.pid == *pid)
                    .map_or(String::new(), |p| p.name.clone());
                format!("end {name} ({pid})? y/n")
            }
            Mode::Help => "? help".into(),
            Mode::Normal if !self.notice.is_empty() => self.notice.clone(),
            Mode::Normal if !self.filter.is_empty() => format!("/{}  Escape clears", self.filter),
            Mode::Normal => "? help".into(),
        };
        let right = format!(
            "{} processes  sort: {}",
            self.rows().len(),
            self.sort.label()
        );
        (left, right)
    }
}
/// Bytes as a short human figure.
pub fn memory(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p(pid: u32, name: &str, cpu: f32, memory: u64) -> Process {
        Process {
            pid,
            name: name.into(),
            cpu,
            memory,
            accessible: true,
        }
    }
    fn sample() -> Tasks {
        let mut t = Tasks::default();
        t.update(vec![
            p(10, "code.exe", 5.0, 300 << 20),
            p(20, "wezterm.exe", 1.0, 900 << 20),
            p(30, "System", 0.0, 10 << 20),
        ]);
        t
    }
    #[test]
    fn sorts_and_cycles() {
        let mut t = sample();
        assert_eq!(t.rows()[0].pid, 10);
        t.key(Key::Char('s'), 10);
        assert_eq!(t.rows()[0].pid, 20);
        t.key(Key::Char('s'), 10);
        assert_eq!(t.rows()[0].name, "code.exe");
        t.key(Key::Char('s'), 10);
        assert_eq!(t.rows()[0].pid, 10);
    }
    #[test]
    fn cursor_follows_process_across_refresh() {
        let mut t = sample();
        t.key(Key::Char('j'), 10);
        assert_eq!(t.selected().unwrap().pid, 20);
        t.update(vec![
            p(20, "wezterm.exe", 9.0, 1),
            p(10, "code.exe", 1.0, 1),
        ]);
        assert_eq!(t.cursor, 0);
        assert_eq!(t.selected().unwrap().pid, 20);
    }
    #[test]
    fn filter_and_kill_flow() {
        let mut t = sample();
        assert_eq!(t.key(Key::Char('/'), 10), Action::None);
        for c in "wez".chars() {
            t.key(Key::Char(c), 10);
        }
        assert_eq!(t.rows().len(), 1);
        assert_eq!(t.status().0, "/wez");
        t.key(Key::Enter, 10);
        assert_eq!(t.mode, Mode::Normal);
        t.key(Key::Char('x'), 10);
        assert_eq!(t.mode, Mode::Confirm(20));
        assert_eq!(t.key(Key::Char('n'), 10), Action::None);
        t.key(Key::Char('x'), 10);
        assert_eq!(t.key(Key::Char('y'), 10), Action::Kill(20));
        t.key(Key::Char('/'), 10);
        t.key(Key::Escape, 10);
        assert_eq!(t.rows().len(), 3);
        assert_eq!(t.key(Key::Char('q'), 10), Action::Quit);
    }
    #[test]
    fn protected_process_and_gg() {
        let mut t = Tasks::default();
        let mut system = p(4, "System", 0.0, 0);
        system.accessible = false;
        t.update(vec![system, p(5, "a.exe", 1.0, 0)]);
        t.key(Key::Char('G'), 10);
        assert_eq!(t.selected().unwrap().pid, 4);
        t.key(Key::Char('x'), 10);
        assert_eq!(t.mode, Mode::Normal);
        assert_eq!(t.status().0, "System cannot be ended");
        t.key(Key::Char('g'), 10);
        t.key(Key::Char('g'), 10);
        assert_eq!(t.cursor, 0);
        assert_eq!(memory(1536 << 20), "1.5 GB");
        assert_eq!(memory(300 << 20), "300 MB");
    }
}
