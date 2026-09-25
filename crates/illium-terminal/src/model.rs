use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{Config, Osc52},
    vte::ansi::Processor,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_CLIPBOARD_BYTES: usize = 65536;
const MAX_CLIPBOARD_WRITES: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: usize,
    pub rows: usize,
}
impl Size {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols: cols.clamp(2, 512),
            rows: rows.clamp(1, 256),
        }
    }
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}
#[derive(Clone, Default)]
pub struct Events(Arc<Mutex<Vec<Event>>>);
impl EventListener for Events {
    fn send_event(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}
pub struct Model {
    pub term: Term<Events>,
    parser: Processor,
    events: Events,
    pub error: Option<String>,
    pub exited: bool,
    clipboard_writes: VecDeque<String>,
    pub clipboard_error: Option<&'static str>,
}
impl Model {
    pub fn new(size: Size, history: usize) -> Self {
        let events = Events::default();
        Self {
            term: Term::new(
                Config {
                    scrolling_history: history,
                    osc52: Osc52::OnlyCopy,
                    kitty_keyboard: false,
                    ..Config::default()
                },
                &size,
                events.clone(),
            ),
            parser: Processor::new(),
            events,
            error: None,
            exited: false,
            clipboard_writes: VecDeque::new(),
            clipboard_error: None,
        }
    }
    pub fn configure(&mut self, history: usize, osc52_copy: bool) {
        self.term.set_options(Config {
            scrolling_history: history,
            osc52: if osc52_copy {
                Osc52::OnlyCopy
            } else {
                Osc52::Disabled
            },
            kitty_keyboard: false,
            ..Config::default()
        });
        if !osc52_copy {
            self.clipboard_writes.clear();
            self.clipboard_error = None;
        }
    }
    pub fn take_clipboard_write(&mut self) -> Option<String> {
        self.clipboard_writes.pop_front()
    }
    fn drain_events(&mut self) -> Vec<Event> {
        let events = std::mem::take(&mut *self.events.0.lock().unwrap());
        events
            .into_iter()
            .filter(|event| {
                if let Event::ClipboardStore(_, text) = event {
                    if text.len() > MAX_CLIPBOARD_BYTES {
                        self.clipboard_error = Some("OSC 52 copy exceeds 64 KiB");
                    } else if text.contains('\0') {
                        self.clipboard_error = Some("OSC 52 copy contains NUL");
                    } else {
                        // Keep recent copies without blocking the PTY reader.
                        if self.clipboard_writes.len() == MAX_CLIPBOARD_WRITES {
                            self.clipboard_writes.pop_front();
                            self.clipboard_error =
                                Some("OSC 52 copy queue overflow; oldest copy dropped");
                        }
                        self.clipboard_writes.push_back(text.clone());
                    }
                    false
                } else {
                    !matches!(event, Event::ClipboardLoad(..))
                }
            })
            .collect()
    }
    /// Caller feeds bounded chunks and drains events before the next chunk.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.parser.advance(&mut self.term, bytes);
        self.drain_events()
    }
    /// A broken application must not freeze the screen by forgetting ESU.
    /// The UI arms a timer only while a synchronized update is pending.
    pub fn expire_sync(&mut self) -> Vec<Event> {
        if self
            .parser
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|at| at <= std::time::Instant::now())
        {
            self.parser.stop_sync(&mut self.term);
        }
        self.drain_events()
    }
    pub fn sync_pending(&self) -> bool {
        self.parser.sync_timeout().sync_timeout().is_some()
    }
    pub fn point(&self, col: usize, row: usize) -> Point {
        Point::new(
            Line(
                row.min(self.term.screen_lines() - 1) as i32
                    - self.term.grid().display_offset() as i32,
            ),
            Column(col.min(self.term.columns() - 1)),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        index::Side,
        selection::{Selection, SelectionType},
        term::TermMode,
    };
    #[test]
    fn osc52_unicode_fragmented_and_successive_writes() {
        let mut m = Model::new(Size::new(80, 24), 0);
        // héllo, newline, emoji. ST and BEL terminators, clipboard and selection.
        for byte in b"\x1b]52;c;aMOpbGxvCvCfmIA=\x1b\\" {
            assert!(m.feed(&[*byte]).is_empty());
        }
        m.feed(b"\x1b]52;p;dHdv\x07\x1b]52;c;dGhyZWU=\x07");
        assert_eq!(m.take_clipboard_write().as_deref(), Some("héllo\n😀"));
        assert_eq!(m.take_clipboard_write().as_deref(), Some("two"));
        assert_eq!(m.take_clipboard_write().as_deref(), Some("three"));
        assert!(m.take_clipboard_write().is_none());
    }
    #[test]
    fn osc52_reads_invalid_data_and_disable() {
        let mut m = Model::new(Size::new(80, 24), 0);
        for sequence in [
            "\x1b]52;c;?\x07",
            "\x1b]52;p;?\x1b\\",
            "\x1b]52;c;%%%\x07",
            "\x1b]52;c;/w==\x07",
        ] {
            assert!(m.feed(sequence.as_bytes()).is_empty());
            assert!(m.take_clipboard_write().is_none());
        }
        m.feed(b"\x1b]52;c;AA==\x07");
        assert!(m.clipboard_error.take().is_some());
        assert!(m.take_clipboard_write().is_none());
        m.feed(b"\x1b]52;c;YQ==\x07");
        m.configure(100, false);
        assert!(m.take_clipboard_write().is_none());
        assert!(m.feed(b"\x1b]52;c;YQ==\x07").iter().all(|event| !matches!(
            event,
            Event::ClipboardStore(..) | Event::ClipboardLoad(..) | Event::PtyWrite(..)
        )));
        assert!(m.take_clipboard_write().is_none());
        m.configure(100, true);
        m.feed(b"\x1b]52;c;YQ==\x07");
        assert_eq!(m.take_clipboard_write().as_deref(), Some("a"));
    }
    #[test]
    fn osc52_size_and_queue_are_bounded() {
        let mut m = Model::new(Size::new(80, 24), 0);
        for (suffix, accepted) in [("YQ==", true), ("YWE=", false)] {
            let sequence = format!("\x1b]52;c;{}{suffix}\x07", "YWFh".repeat(21845));
            for chunk in sequence.as_bytes().chunks(16384) {
                m.feed(chunk);
            }
            assert_eq!(
                m.take_clipboard_write().map(|s| s.len()),
                accepted.then_some(65536)
            );
            assert_eq!(m.clipboard_error.take().is_some(), !accepted);
        }
        m.feed(b"\x1b]52;c;b2xk\x07");
        for _ in 0..MAX_CLIPBOARD_WRITES {
            m.feed(b"\x1b]52;c;bmV3\x07");
        }
        assert!(m.clipboard_error.is_some());
        for _ in 0..MAX_CLIPBOARD_WRITES {
            assert_eq!(m.take_clipboard_write().as_deref(), Some("new"));
        }
        assert!(m.take_clipboard_write().is_none());
    }
    #[test]
    fn osc52_synchronized_update_end_and_timeout() {
        let mut m = Model::new(Size::new(80, 24), 0);
        m.feed(b"\x1b[?2026h\x1b]52;c;YQ==\x07\x1b[?2026l");
        assert_eq!(m.take_clipboard_write().as_deref(), Some("a"));
        m.feed(b"\x1b[?2026h\x1b]52;c;Yg==\x07");
        std::thread::sleep(std::time::Duration::from_millis(200));
        m.expire_sync();
        assert_eq!(m.take_clipboard_write().as_deref(), Some("b"));
    }
    #[test]
    fn vt_unicode_alternate_resize_and_reports() {
        let mut m = Model::new(Size::new(80, 24), 100);
        m.feed("é界\x1b[31mX".as_bytes());
        assert_eq!(m.term.grid()[Line(0)][Column(0)].c, 'é');
        assert_eq!(m.term.grid()[Line(0)][Column(1)].c, '界');
        let events = m.feed(b"\x1b[6n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Event::PtyWrite(s) if s=="\x1b[1;5R"))
        );
        m.feed(b"\x1b[?1049h\x1b[?2004h");
        assert!(
            m.term
                .mode()
                .contains(TermMode::ALT_SCREEN | TermMode::BRACKETED_PASTE)
        );
        m.feed(b"\x1b[?1049l");
        assert_eq!(m.term.grid()[Line(0)][Column(0)].c, 'é');
        m.term.resize(Size::new(100, 30));
        assert_eq!(m.term.columns(), 100);
    }
    #[test]
    fn synchronized_output_has_a_bounded_timeout() {
        let mut m = Model::new(Size::new(10, 2), 0);
        m.feed(b"\x1b[?2026hhello");
        assert!(m.sync_pending());
        assert_eq!(m.term.grid()[Line(0)][Column(0)].c, ' ');
        std::thread::sleep(std::time::Duration::from_millis(200));
        m.expire_sync();
        assert!(!m.sync_pending());
        assert_eq!(m.term.grid()[Line(0)][Column(0)].c, 'h');
        m.feed(b"\x1b[?2026h!\x1b[?2026l");
        assert!(!m.sync_pending());
    }
    #[test]
    fn chunked_utf8_and_selection() {
        let mut m = Model::new(Size::new(10, 2), 2);
        for byte in "héllo".as_bytes() {
            m.feed(&[*byte]);
        }
        let mut s = Selection::new(SelectionType::Simple, m.point(0, 0), Side::Left);
        s.update(m.point(4, 0), Side::Right);
        m.term.selection = Some(s);
        assert_eq!(m.term.selection_to_string().unwrap(), "héllo");
        for _ in 0..20 {
            m.feed(b"\r\nline");
        }
        assert!(m.term.total_lines() <= 4);
    }
}
