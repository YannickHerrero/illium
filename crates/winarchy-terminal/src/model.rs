use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{Config, Osc52},
    vte::ansi::Processor,
};
use std::sync::{Arc, Mutex};
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
}
impl Model {
    pub fn new(size: Size, history: usize) -> Self {
        let events = Events::default();
        Self {
            term: Term::new(
                Config {
                    scrolling_history: history,
                    osc52: Osc52::Disabled,
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
        }
    }
    /// Caller feeds bounded chunks and drains events before the next chunk.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.parser.advance(&mut self.term, bytes);
        self.events.0.lock().unwrap().drain(..).collect()
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
