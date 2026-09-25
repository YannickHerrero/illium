//! Placement memory across daemon restarts. Windows outlive Illium, so the
//! HWND stays a valid key; the process behind it is checked against reuse.
use crate::layout::Rect;
use serde::{Deserialize, Serialize};
use std::path::Path;
pub const MAX_STATE_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub id: isize,
    pub pid: u32,
    pub exe: String,
    /// Files written before spaces existed load into the default space 0.
    #[serde(default)]
    pub space: u32,
    pub workspace: u8,
    pub floating: bool,
    pub fullscreen: bool,
    pub restore: Rect,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub active: u8,
    pub recent: u8,
    pub monitors: [usize; 9],
    /// Tile order is the vector order.
    pub clients: Vec<Placement>,
    /// The top-level workspace fields belong to this space.
    #[serde(default)]
    pub space: u32,
    #[serde(default)]
    pub recent_space: u32,
    #[serde(default)]
    pub spaces: Vec<SavedSpace>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSpace {
    pub id: u32,
    pub name: String,
    pub active: u8,
    pub recent: u8,
    pub monitors: [usize; 9],
}
impl State {
    /// A missing or unreadable file is not an error: the desktop starts fresh.
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = crate::files::read_bounded(path, MAX_STATE_BYTES).ok()?;
        let mut state: Self = serde_json::from_slice(&bytes).ok()?;
        if !(1..=9).contains(&state.active) {
            state.active = 1;
        }
        if !(1..=9).contains(&state.recent) {
            state.recent = state.active;
        }
        state.clients.retain(|c| (1..=9).contains(&c.workspace));
        let mut names = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        state.spaces.retain(|s| {
            crate::command::space_name(&s.name).is_ok()
                && ids.insert(s.id)
                && names.insert(s.name.clone())
        });
        state.spaces.truncate(crate::model::MAX_SPACES);
        for s in &mut state.spaces {
            if !(1..=9).contains(&s.active) {
                s.active = 1;
            }
            if !(1..=9).contains(&s.recent) {
                s.recent = s.active;
            }
        }
        if !state.spaces.iter().any(|s| s.id == state.space) {
            state.spaces.clear();
            state.space = 0;
        }
        if !state.spaces.iter().any(|s| s.id == state.recent_space) {
            state.recent_space = state.space;
        }
        let space = state.space;
        let known: Vec<u32> = state.spaces.iter().map(|s| s.id).collect();
        for c in &mut state.clients {
            if !known.contains(&c.space) {
                c.space = space;
            }
        }
        Some(state)
    }
    /// Keeps only the entries whose window still belongs to the same process.
    pub fn retain_alive(&mut self, process: impl Fn(isize) -> Option<(u32, String)>) {
        self.clients
            .retain(|c| process(c.id).is_some_and(|(pid, exe)| pid == c.pid && exe == c.exe));
    }
    pub fn placement(&self, id: isize) -> Option<&Placement> {
        self.clients.iter().find(|c| c.id == id)
    }
    /// Saved tile index; unknown windows sort after every remembered one.
    pub fn order(&self, id: isize) -> usize {
        self.clients
            .iter()
            .position(|c| c.id == id)
            .unwrap_or(usize::MAX)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    /// Written next to the file then renamed, so a reader never sees a torn file.
    pub fn save(json: &str, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn placement(id: isize, workspace: u8) -> Placement {
        Placement {
            id,
            pid: 40 + id as u32,
            exe: format!("C:\\app{id}.exe"),
            space: 0,
            workspace,
            floating: id == 2,
            fullscreen: false,
            restore: Rect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            },
        }
    }
    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("illium-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }
    #[test]
    fn round_trip() {
        let path = temp("round.json");
        let state = State {
            active: 3,
            recent: 1,
            monitors: [0; 9],
            clients: vec![placement(1, 3), placement(2, 5)],
            space: 1,
            recent_space: 0,
            spaces: vec![saved(0, "dev"), saved(1, "perso")],
        };
        State::save(&state.to_json(), &path).unwrap();
        assert_eq!(State::load(&path).unwrap(), state);
        assert!(!path.with_extension("json.tmp").exists());
    }
    #[test]
    fn missing_or_invalid_file_starts_fresh() {
        assert!(State::load(&temp("absent.json")).is_none());
        let path = temp("garbage.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(State::load(&path).is_none());
    }
    #[test]
    fn out_of_range_values_are_normalized() {
        let path = temp("range.json");
        let mut state = State {
            active: 12,
            recent: 0,
            monitors: [0; 9],
            clients: vec![placement(1, 3), placement(2, 0)],
            ..Default::default()
        };
        State::save(&state.to_json(), &path).unwrap();
        state = State::load(&path).unwrap();
        assert_eq!((state.active, state.recent), (1, 1));
        assert_eq!(state.clients.len(), 1);
    }
    fn saved(id: u32, name: &str) -> SavedSpace {
        SavedSpace {
            id,
            name: name.into(),
            active: 2,
            recent: 1,
            monitors: [0; 9],
        }
    }
    #[test]
    fn files_without_spaces_load_into_the_default_space() {
        let path = temp("legacy.json");
        std::fs::write(
            &path,
            r#"{"active":2,"recent":1,"monitors":[0,0,0,0,0,0,0,0,0],"clients":[{"id":1,"pid":41,"exe":"a","workspace":2,"floating":false,"fullscreen":false,"restore":{"x":0,"y":0,"w":1,"h":1}}]}"#,
        )
        .unwrap();
        let state = State::load(&path).unwrap();
        assert!(state.spaces.is_empty());
        assert_eq!((state.space, state.clients[0].space), (0, 0));
    }
    #[test]
    fn invalid_spaces_are_dropped() {
        let path = temp("spaces.json");
        let mut perso = saved(4, "perso");
        perso.active = 0;
        let mut state = State {
            active: 1,
            recent: 1,
            clients: vec![Placement {
                space: 9,
                ..placement(1, 3)
            }],
            space: 4,
            recent_space: 7,
            spaces: vec![saved(0, "dev"), saved(3, "dev"), saved(5, "a b"), perso],
            ..Default::default()
        };
        State::save(&state.to_json(), &path).unwrap();
        state = State::load(&path).unwrap();
        assert_eq!(
            state.spaces.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 4]
        );
        assert_eq!((state.spaces[1].active, state.recent_space), (1, 4));
        assert_eq!(state.clients[0].space, 4);
        state.space = 8;
        State::save(&state.to_json(), &path).unwrap();
        state = State::load(&path).unwrap();
        assert!(state.spaces.is_empty());
        assert_eq!((state.space, state.clients[0].space), (0, 0));
    }
    #[test]
    fn retains_only_windows_of_the_same_process() {
        let mut state = State {
            clients: vec![placement(1, 1), placement(2, 1), placement(3, 1)],
            ..Default::default()
        };
        state.retain_alive(|id| match id {
            1 => Some((41, "C:\\app1.exe".into())),
            2 => Some((99, "C:\\app2.exe".into())),
            _ => None,
        });
        assert_eq!(
            state.clients.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![1]
        );
    }
    #[test]
    fn order_places_unknown_windows_last() {
        let state = State {
            clients: vec![placement(7, 1), placement(5, 1)],
            ..Default::default()
        };
        let mut ids = vec![5, 9, 7];
        ids.sort_by_key(|id| state.order(*id));
        assert_eq!(ids, vec![7, 5, 9]);
        assert_eq!(state.placement(5).map(|p| p.pid), Some(45));
    }
}
