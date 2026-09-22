use crate::layout::Rect;
#[derive(Debug, Clone)]
pub struct Client {
    pub id: isize,
    pub generation: usize,
    pub workspace: u8,
    pub floating: bool,
    pub fullscreen: bool,
    /// Hidden by Winarchy because its workspace is inactive.
    pub hidden: bool,
    /// Rectangle before the client was parked off screen.
    pub parked: Option<Rect>,
    pub restore: Rect,
}
#[derive(Default)]
pub struct Model {
    pub clients: Vec<Client>,
    pub active: u8,
    pub recent: u8,
    pub focused: Option<isize>,
    pub monitors: [usize; 9],
    pub splits: [crate::layout::Splits; 9],
}
impl Model {
    pub fn new() -> Self {
        Self {
            active: 1,
            recent: 1,
            ..Self::default()
        }
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
            .find(|n| self.clients.iter().any(|c| c.workspace == *n))
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
}
