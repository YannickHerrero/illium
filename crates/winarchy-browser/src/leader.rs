//! Platform-independent leader key map and state machine.
use std::time::{Duration, Instant};

pub const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Address,
    Back,
    Forward,
    Find,
    Reload,
    Bookmark,
    Home,
    CopyUrl,
    HardReload,
    Stop,
    DevTools,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Blocking,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Root,
    Navigation,
    Page,
    Zoom,
    Blocking,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Action(Action),
    Menu(Menu),
}
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub key: char,
    pub label: &'static str,
    pub target: Target,
}
macro_rules! entries {
    ($($key:literal, $label:literal, $kind:ident($value:ident);)*) => {
        &[$(Entry { key: $key, label: $label, target: Target::$kind($kind::$value) }),*]
    };
}
impl Menu {
    pub fn title(self) -> &'static str {
        match self {
            Self::Root => "Principal",
            Self::Navigation => "Navigation",
            Self::Page => "Page",
            Self::Zoom => "Zoom",
            Self::Blocking => "Blocage",
        }
    }
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Root => "",
            Self::Navigation => "n",
            Self::Page => "p",
            Self::Zoom => "z",
            Self::Blocking => "b",
        }
    }
    pub fn entries(self) -> &'static [Entry] {
        match self {
            Self::Root => entries! {
                'l', "Adresse / recherche", Action(Address);
                'h', "Page précédente", Action(Back);
                'f', "Rechercher dans la page", Action(Find);
                'r', "Recharger", Action(Reload);
                'd', "Ajouter aux favoris", Action(Bookmark);
                'n', "Navigation", Menu(Navigation);
                'p', "Page", Menu(Page);
                'z', "Zoom", Menu(Zoom);
                'b', "Blocage", Menu(Blocking);
            },
            Self::Navigation => entries! {
                'p', "Page précédente", Action(Back);
                's', "Page suivante", Action(Forward);
                'a', "Accueil", Action(Home);
            },
            Self::Page => entries! {
                'u', "Copier l’URL", Action(CopyUrl);
                'r', "Recharger sans cache", Action(HardReload);
                's', "Arrêter le chargement", Action(Stop);
                'i', "Outils de développement", Action(DevTools);
            },
            Self::Zoom => entries! {
                '+', "Agrandir (+ ou =)", Action(ZoomIn);
                '-', "Réduire", Action(ZoomOut);
                '0', "Réinitialiser à 100 %", Action(ZoomReset);
            },
            Self::Blocking => entries! { 'b', "Activer / désactiver ce site", Action(Blocking); },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Leader,
    Escape,
    Backspace,
    Character(char),
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Changed,
    Cancelled,
    Execute(Action),
}
#[derive(Default)]
pub struct Leader {
    menu: Option<Menu>,
    deadline: Option<Instant>,
}
impl Leader {
    pub fn menu(&self) -> Option<Menu> {
        self.menu
    }
    pub fn cancel(&mut self) {
        self.menu = None;
        self.deadline = None;
    }
    pub fn remaining(&self, now: Instant) -> Duration {
        self.deadline
            .map(|d| d.saturating_duration_since(now))
            .unwrap_or_default()
    }
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|d| now >= d) {
            self.cancel();
            true
        } else {
            false
        }
    }
    pub fn input(&mut self, key: Key, repeat: bool, now: Instant) -> Outcome {
        self.expire(now);
        if repeat {
            return if self.menu.is_some() || key == Key::Leader {
                Outcome::Changed
            } else {
                Outcome::Pass
            };
        }
        if key == Key::Leader {
            if self.menu.is_some() {
                self.cancel();
                return Outcome::Pass;
            }
            self.menu = Some(Menu::Root);
            self.deadline = Some(now + TIMEOUT);
            return Outcome::Changed;
        }
        let Some(menu) = self.menu else {
            return Outcome::Pass;
        };
        if key == Key::Backspace && menu != Menu::Root {
            self.menu = Some(Menu::Root);
            self.deadline = Some(now + TIMEOUT);
            return Outcome::Changed;
        }
        let key = match key {
            Key::Character('=') if menu == Menu::Zoom => Key::Character('+'),
            key => key,
        };
        if let Some(entry) = menu.entries().iter().find(|e| key == Key::Character(e.key)) {
            match entry.target {
                Target::Menu(menu) => {
                    self.menu = Some(menu);
                    self.deadline = Some(now + TIMEOUT);
                    Outcome::Changed
                }
                Target::Action(action) => {
                    self.cancel();
                    Outcome::Execute(action)
                }
            }
        } else {
            self.cancel();
            Outcome::Cancelled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_entry_is_unique_and_reachable() {
        for menu in [
            Menu::Root,
            Menu::Navigation,
            Menu::Page,
            Menu::Zoom,
            Menu::Blocking,
        ] {
            let mut keys = std::collections::HashSet::new();
            for entry in menu.entries() {
                assert!(keys.insert(entry.key));
                let now = Instant::now();
                let mut leader = Leader::default();
                leader.input(Key::Leader, false, now);
                if menu != Menu::Root {
                    leader.input(
                        Key::Character(menu.prefix().chars().next().unwrap()),
                        false,
                        now,
                    );
                }
                let result = leader.input(Key::Character(entry.key), false, now);
                match entry.target {
                    Target::Action(action) => {
                        assert_eq!(result, Outcome::Execute(action));
                        assert_eq!(leader.menu(), None);
                    }
                    Target::Menu(menu) => {
                        assert_eq!(result, Outcome::Changed);
                        assert_eq!(leader.menu(), Some(menu));
                    }
                }
            }
        }
    }
    #[test]
    fn timeout_back_cancel_and_passthrough() {
        let now = Instant::now();
        let mut l = Leader::default();
        assert_eq!(l.input(Key::Character('l'), false, now), Outcome::Pass);
        l.input(Key::Leader, false, now);
        l.input(Key::Character('n'), false, now + Duration::from_secs(2));
        assert!(!l.expire(now + TIMEOUT));
        assert_eq!(
            l.input(Key::Backspace, false, now + TIMEOUT),
            Outcome::Changed
        );
        assert_eq!(l.menu(), Some(Menu::Root));
        assert_eq!(l.input(Key::Leader, false, now + TIMEOUT), Outcome::Pass);
        assert_eq!(l.menu(), None);
        for key in [Key::Escape, Key::Other, Key::Backspace, Key::Character('x')] {
            l.input(Key::Leader, false, now);
            assert_eq!(l.input(key, false, now), Outcome::Cancelled);
        }
        l.input(Key::Leader, false, now);
        assert!(l.expire(now + TIMEOUT));
        assert_eq!(
            l.input(Key::Character('l'), false, now + TIMEOUT),
            Outcome::Pass
        );
    }
    #[test]
    fn repeats_do_not_execute_or_extend_timeout_and_zoom_alias_works() {
        let now = Instant::now();
        let mut l = Leader::default();
        l.input(Key::Leader, false, now);
        l.input(Key::Leader, true, now + Duration::from_secs(2));
        l.input(Key::Character('r'), true, now + Duration::from_secs(2));
        assert!(l.expire(now + TIMEOUT));
        l.input(Key::Leader, false, now);
        l.input(Key::Character('z'), false, now);
        assert_eq!(
            l.input(Key::Character('='), false, now),
            Outcome::Execute(Action::ZoomIn)
        );
    }
}
