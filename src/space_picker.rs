//! Space picker model: one row per named space, keyboard navigation and the
//! inline name prompt. Pure state: switching, renaming and deleting spaces
//! live in the platform layer.
use crate::command::SpaceCommand;

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub current: bool,
    /// Application names of its windows, without duplicates.
    pub apps: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Mode {
    #[default]
    Browse,
    Create(String),
    Rename(String),
    /// Waiting for the second `D` that confirms the deletion.
    Delete,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Escape,
    Confirm,
    Next,
    Previous,
    Backspace,
    Text(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    None,
    Close,
    /// A prompt stays open until the command succeeds and new rows arrive.
    Run(SpaceCommand),
}
#[derive(Default)]
pub struct Model {
    pub rows: Vec<Row>,
    pub selected: usize,
    pub mode: Mode,
    pub error: Option<String>,
}
impl Model {
    pub fn new(rows: Vec<Row>, selected: usize) -> Self {
        Self {
            selected: selected.min(rows.len().saturating_sub(1)),
            rows,
            ..Self::default()
        }
    }
    /// After a rename or a deletion: back to browsing, selection kept in range.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        self.rows = rows;
        self.mode = Mode::Browse;
    }
    pub fn select(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }
    fn name(&self) -> String {
        self.rows
            .get(self.selected)
            .map(|r| r.name.clone())
            .unwrap_or_default()
    }
    pub fn action(&mut self, action: Action) -> Outcome {
        self.error = None;
        let count = self.rows.len().max(1);
        let letter = |text: &str| text.to_lowercase();
        match (&mut self.mode, action) {
            (Mode::Browse, Action::Escape) => Outcome::Close,
            (Mode::Browse, Action::Confirm) => match self.rows.get(self.selected) {
                Some(row) if !row.current => Outcome::Run(SpaceCommand::Switch(row.name.clone())),
                _ => Outcome::Close,
            },
            (Mode::Browse, Action::Next) => {
                self.selected = (self.selected + 1) % count;
                Outcome::None
            }
            (Mode::Browse, Action::Previous) => {
                self.selected = (self.selected + count - 1) % count;
                Outcome::None
            }
            (Mode::Browse, Action::Text(text)) => {
                match letter(&text).as_str() {
                    "n" => self.mode = Mode::Create(String::new()),
                    "e" => self.mode = Mode::Rename(self.name()),
                    "d" => self.mode = Mode::Delete,
                    _ => {}
                }
                Outcome::None
            }
            (Mode::Browse, Action::Backspace) => Outcome::None,
            (Mode::Delete, Action::Text(text)) if letter(&text) == "d" => {
                self.mode = Mode::Browse;
                Outcome::Run(SpaceCommand::Delete(self.name()))
            }
            (Mode::Delete, _) => {
                self.mode = Mode::Browse;
                Outcome::None
            }
            (Mode::Create(_) | Mode::Rename(_), Action::Escape) => {
                self.mode = Mode::Browse;
                Outcome::None
            }
            (Mode::Create(buffer) | Mode::Rename(buffer), Action::Backspace) => {
                buffer.pop();
                Outcome::None
            }
            (Mode::Create(buffer) | Mode::Rename(buffer), Action::Text(text)) => {
                for c in text.chars() {
                    if buffer.chars().count() < 24 && !c.is_whitespace() && !c.is_control() {
                        buffer.push(c);
                    }
                }
                Outcome::None
            }
            (Mode::Create(buffer), Action::Confirm) => {
                Outcome::Run(SpaceCommand::Create(buffer.clone()))
            }
            (Mode::Rename(buffer), Action::Confirm) => {
                let new = buffer.clone();
                Outcome::Run(SpaceCommand::Rename(self.name(), new))
            }
            (Mode::Create(_) | Mode::Rename(_), Action::Next | Action::Previous) => Outcome::None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rows() -> Vec<Row> {
        ["dev", "perso", "docs"]
            .iter()
            .enumerate()
            .map(|(i, name)| Row {
                name: (*name).into(),
                current: i == 0,
                apps: vec![],
            })
            .collect()
    }
    fn text(s: &str) -> Action {
        Action::Text(s.into())
    }
    #[test]
    fn browsing_and_switching() {
        let mut m = Model::new(rows(), 1);
        assert_eq!(
            m.action(Action::Confirm),
            Outcome::Run(SpaceCommand::Switch("perso".into()))
        );
        m.action(Action::Next);
        m.action(Action::Next);
        assert_eq!(m.selected, 0);
        assert_eq!(m.action(Action::Confirm), Outcome::Close);
        m.action(Action::Previous);
        assert_eq!(m.selected, 2);
        assert_eq!(m.action(text("x")), Outcome::None);
        assert_eq!(m.action(Action::Escape), Outcome::Close);
        assert_eq!(Model::new(rows(), 9).selected, 2);
    }
    #[test]
    fn creating_keeps_the_prompt_until_new_rows() {
        let mut m = Model::new(rows(), 0);
        m.action(text("N"));
        for s in ["w", "o r", "k"] {
            m.action(text(s));
        }
        m.action(Action::Backspace);
        m.action(text("x"));
        assert_eq!(
            m.action(Action::Confirm),
            Outcome::Run(SpaceCommand::Create("worx".into()))
        );
        assert_eq!(m.mode, Mode::Create("worx".into()));
        m.action(Action::Escape);
        assert_eq!(m.mode, Mode::Browse);
    }
    #[test]
    fn renaming_starts_from_the_selected_name() {
        let mut m = Model::new(rows(), 1);
        m.action(text("e"));
        assert_eq!(m.mode, Mode::Rename("perso".into()));
        m.action(text("2"));
        assert_eq!(
            m.action(Action::Confirm),
            Outcome::Run(SpaceCommand::Rename("perso".into(), "perso2".into()))
        );
        let mut renamed = rows();
        renamed[1].name = "perso2".into();
        m.set_rows(renamed);
        assert_eq!((m.mode.clone(), m.selected), (Mode::Browse, 1));
    }
    #[test]
    fn deleting_needs_a_second_d() {
        let mut m = Model::new(rows(), 2);
        m.action(text("d"));
        assert_eq!(m.action(Action::Next), Outcome::None);
        assert_eq!((m.mode.clone(), m.selected), (Mode::Browse, 2));
        m.action(text("d"));
        assert_eq!(
            m.action(text("D")),
            Outcome::Run(SpaceCommand::Delete("docs".into()))
        );
        m.set_rows(rows()[..2].to_vec());
        assert_eq!(m.selected, 1);
    }
}
