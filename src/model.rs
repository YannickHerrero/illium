use crate::layout::{Rect, Splits};
#[derive(Debug, Clone)]
pub struct Client {
    pub id: isize,
    pub generation: usize,
    pub space: u32,
    pub workspace: u8,
    pub floating: bool,
    pub fullscreen: bool,
    /// Hidden by Winarchy because its workspace is inactive.
    pub hidden: bool,
    /// Rectangle before the client was parked off screen.
    pub parked: Option<Rect>,
    pub restore: Rect,
}
impl Client {
    pub fn on(&self, space: u32, workspace: u8) -> bool {
        self.space == space && self.workspace == workspace
    }
}
/// The workspace fields of the current space live on `Model`: a space keeps
/// its own copy only while inactive, and switching swaps them.
#[derive(Debug, Clone)]
pub struct Space {
    pub id: u32,
    pub name: String,
    pub active: u8,
    pub recent: u8,
    pub monitors: [usize; 9],
    pub splits: [Splits; 9],
    pub focused: Option<isize>,
}
impl Space {
    pub fn new(id: u32, name: String) -> Self {
        Self {
            id,
            name,
            active: 1,
            recent: 1,
            monitors: [0; 9],
            splits: Default::default(),
            focused: None,
        }
    }
}
pub const DEFAULT_SPACE: &str = "dev";
pub const MAX_SPACES: usize = 32;
#[derive(Default)]
pub struct Model {
    pub clients: Vec<Client>,
    pub active: u8,
    pub recent: u8,
    pub focused: Option<isize>,
    pub monitors: [usize; 9],
    pub splits: [Splits; 9],
    pub space: u32,
    pub recent_space: u32,
    /// Creation order, never empty.
    pub spaces: Vec<Space>,
}
impl Model {
    pub fn new() -> Self {
        Self {
            active: 1,
            recent: 1,
            spaces: vec![Space::new(0, DEFAULT_SPACE.into())],
            ..Self::default()
        }
    }
    /// On the active workspace of the current space.
    pub fn shown(&self, c: &Client) -> bool {
        c.on(self.space, self.active)
    }
    pub fn occupied(&self, n: u8) -> bool {
        self.clients.iter().any(|c| c.on(self.space, n))
    }
    pub fn switch(&mut self, n: u8) {
        if (1..=9).contains(&n) && n != self.active {
            self.recent = self.active;
            self.active = n;
        }
    }
    pub fn next(&self) -> u8 {
        (1..=9)
            .map(|i| (self.active - 1 + i) % 9 + 1)
            .find(|n| self.occupied(*n))
            .unwrap_or(self.active)
    }
    pub fn move_to(&mut self, id: isize, n: u8, follow: bool) {
        if !(1..=9).contains(&n) {
            return;
        }
        if let Some(c) = self.clients.iter_mut().find(|c| c.id == id) {
            c.workspace = n;
            if follow {
                self.switch(n);
            }
        }
    }
    pub fn swap(&mut self, a: isize, b: isize) {
        if let (Some(a), Some(b)) = (
            self.clients.iter().position(|c| c.id == a),
            self.clients.iter().position(|c| c.id == b),
        ) {
            self.clients.swap(a, b);
        }
    }
    pub fn space_name(&self) -> &str {
        self.spaces
            .iter()
            .find(|s| s.id == self.space)
            .map_or(DEFAULT_SPACE, |s| &s.name)
    }
    pub fn space_id(&self, name: &str) -> Result<u32, String> {
        self.spaces
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.id)
            .ok_or_else(|| format!("unknown space: {name}"))
    }
    /// Active workspace, recent workspace and home monitors of any space.
    pub fn context(&self, space: &Space) -> (u8, u8, [usize; 9]) {
        if space.id == self.space {
            (self.active, self.recent, self.monitors)
        } else {
            (space.active, space.recent, space.monitors)
        }
    }
    /// The space after the current one in creation order, wrapping around.
    pub fn next_space(&self) -> u32 {
        let index = self
            .spaces
            .iter()
            .position(|s| s.id == self.space)
            .unwrap_or(0);
        self.spaces[(index + 1) % self.spaces.len()].id
    }
    pub fn switch_space(&mut self, id: u32) {
        if id == self.space || !self.spaces.iter().any(|s| s.id == id) {
            return;
        }
        self.exchange(self.space);
        self.exchange(id);
        self.recent_space = self.space;
        self.space = id;
    }
    fn exchange(&mut self, id: u32) {
        if let Some(s) = self.spaces.iter_mut().find(|s| s.id == id) {
            std::mem::swap(&mut self.active, &mut s.active);
            std::mem::swap(&mut self.recent, &mut s.recent);
            std::mem::swap(&mut self.monitors, &mut s.monitors);
            std::mem::swap(&mut self.splits, &mut s.splits);
            std::mem::swap(&mut self.focused, &mut s.focused);
        }
    }
    /// The new space starts empty and becomes current.
    pub fn create_space(&mut self, name: &str) -> Result<(), String> {
        let name = crate::command::space_name(name)?;
        if self.space_id(&name).is_ok() {
            return Err(format!("space {name} already exists"));
        }
        if self.spaces.len() >= MAX_SPACES {
            return Err(format!("at most {MAX_SPACES} spaces"));
        }
        let id = self.spaces.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        self.spaces.push(Space {
            monitors: self.monitors,
            ..Space::new(id, name)
        });
        self.switch_space(id);
        Ok(())
    }
    pub fn rename_space(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = crate::command::space_name(new)?;
        let id = self.space_id(old)?;
        if old != new && self.space_id(&new).is_ok() {
            return Err(format!("space {new} already exists"));
        }
        if let Some(s) = self.spaces.iter_mut().find(|s| s.id == id) {
            s.name = new;
        }
        Ok(())
    }
    /// No window is closed: those of the deleted space join the current one
    /// on the same workspace numbers.
    pub fn delete_space(&mut self, name: &str) -> Result<(), String> {
        let id = self.space_id(name)?;
        if self.spaces.len() == 1 {
            return Err("the last space cannot be deleted".into());
        }
        if id == self.space {
            let fallback = if self.recent_space != id
                && self.spaces.iter().any(|s| s.id == self.recent_space)
            {
                self.recent_space
            } else {
                self.next_space()
            };
            self.switch_space(fallback);
        }
        for c in self.clients.iter_mut().filter(|c| c.space == id) {
            c.space = self.space;
        }
        self.spaces.retain(|s| s.id != id);
        if self.recent_space == id {
            self.recent_space = self.space;
        }
        Ok(())
    }
    /// The window keeps its workspace number; following shows that workspace.
    pub fn move_to_space(&mut self, id: isize, space: u32, follow: bool) {
        if !self.spaces.iter().any(|s| s.id == space) {
            return;
        }
        if let Some(c) = self.clients.iter_mut().find(|c| c.id == id) {
            c.space = space;
            let workspace = c.workspace;
            if follow {
                self.switch_space(space);
                self.switch(workspace);
                self.focused = Some(id);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history() {
        let mut m = Model::new();
        m.switch(2);
        m.switch(2);
        assert_eq!(m.recent, 1);
        m.switch(m.recent);
        assert_eq!((m.active, m.recent), (1, 2));
    }
    #[test]
    fn ordering_and_follow() {
        let mut m = Model::new();
        for id in 1..=3 {
            m.clients.push(Client {
                id,
                generation: id as usize,
                space: 0,
                workspace: 1,
                floating: false,
                fullscreen: false,
                hidden: false,
                parked: None,
                restore: Rect::default(),
            });
        }
        m.swap(1, 3);
        assert_eq!(
            m.clients.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        m.move_to(2, 4, true);
        assert_eq!(m.active, 4);
        assert_eq!(m.recent, 1);
        assert_eq!(m.next(), 1);
    }
    fn client(id: isize, space: u32, workspace: u8) -> Client {
        Client {
            id,
            generation: id as usize,
            space,
            workspace,
            floating: false,
            fullscreen: false,
            hidden: false,
            parked: None,
            restore: Rect::default(),
        }
    }
    #[test]
    fn spaces_keep_their_own_workspaces() {
        let mut m = Model::new();
        m.clients.push(client(1, 0, 1));
        m.switch(3);
        m.monitors[2] = 1;
        m.create_space("perso").unwrap();
        assert_eq!(m.space_name(), "perso");
        assert_eq!((m.active, m.recent), (1, 1));
        assert_eq!(m.monitors[2], 1);
        assert!(!m.shown(&m.clients[0]));
        assert_eq!(m.next(), 1);
        m.clients.push(client(2, 1, 5));
        m.switch(5);
        m.switch_space(0);
        assert_eq!((m.space, m.recent_space, m.active, m.recent), (0, 1, 3, 1));
        assert_eq!(m.next(), 1);
        let perso = m.spaces.iter().find(|s| s.id == 1).unwrap();
        assert_eq!(m.context(perso).0, 5);
        m.focused = Some(1);
        m.switch_space(m.next_space());
        assert_eq!((m.space, m.active, m.focused), (1, 5, None));
        m.switch_space(0);
        assert_eq!(m.focused, Some(1));
        m.switch_space(1);
        assert!(m.shown(&m.clients[1]));
    }
    #[test]
    fn space_names_are_unique_and_valid() {
        let mut m = Model::new();
        assert!(m.create_space("dev").is_err());
        assert!(m.create_space("two words").is_err());
        m.create_space("perso").unwrap();
        assert!(m.rename_space("perso", "dev").is_err());
        m.rename_space("perso", "maison").unwrap();
        m.rename_space("maison", "maison").unwrap();
        assert_eq!(m.space_id("maison"), Ok(1));
        assert!(m.rename_space("absent", "x").is_err());
    }
    #[test]
    fn deleting_a_space_keeps_its_windows() {
        let mut m = Model::new();
        assert!(m.delete_space("dev").is_err());
        m.create_space("perso").unwrap();
        m.clients.push(client(1, 1, 4));
        m.create_space("docs").unwrap();
        m.switch_space(1);
        m.delete_space("perso").unwrap();
        assert_eq!((m.space, m.recent_space), (2, 2));
        assert_eq!(m.clients[0].space, 2);
        assert_eq!(m.clients[0].workspace, 4);
        assert_eq!(m.spaces.len(), 2);
        m.delete_space("dev").unwrap();
        assert_eq!(m.space_name(), "docs");
    }
    #[test]
    fn moving_a_window_to_another_space() {
        let mut m = Model::new();
        m.clients.push(client(1, 0, 6));
        m.create_space("perso").unwrap();
        m.switch_space(0);
        m.move_to_space(1, 1, false);
        assert_eq!(m.space, 0);
        m.move_to_space(1, 0, true);
        m.move_to_space(1, 1, true);
        assert_eq!((m.space, m.active), (1, 6));
        m.move_to_space(1, 9, true);
        assert_eq!(m.clients[0].space, 1);
    }
}
