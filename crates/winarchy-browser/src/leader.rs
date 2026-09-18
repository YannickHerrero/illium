//! Platform-independent leader key map and state machine.
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
}
impl Leader {
    pub fn menu(&self) -> Option<Menu> {
        self.menu
    }
    pub fn cancel(&mut self) {
        self.menu = None;
    }
    pub fn input(&mut self, key: Key, repeat: bool) -> Outcome {
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
            return Outcome::Changed;
        }
        let Some(menu) = self.menu else {
            return Outcome::Pass;
        };
        if key == Key::Escape {
            self.cancel();
            return Outcome::Cancelled;
        }
        if key == Key::Backspace {
            self.menu = Some(Menu::Root);
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
                    Outcome::Changed
                }
                Target::Action(action) => {
                    self.cancel();
                    Outcome::Execute(action)
                }
            }
        } else {
            // Consume unknown keys without closing the persistent menu.
            Outcome::Changed
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
                let mut leader = Leader::default();
                leader.input(Key::Leader, false);
                if menu != Menu::Root {
                    leader.input(Key::Character(menu.prefix().chars().next().unwrap()), false);
                }
                let result = leader.input(Key::Character(entry.key), false);
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
    fn persistent_menu_back_cancel_and_passthrough() {
        let mut l = Leader::default();
        assert_eq!(l.input(Key::Character('l'), false), Outcome::Pass);
        l.input(Key::Leader, false);
        for key in [Key::Other, Key::Backspace, Key::Character('x')] {
            assert_eq!(l.input(key, false), Outcome::Changed);
            assert_eq!(l.menu(), Some(Menu::Root));
        }
        l.input(Key::Character('n'), false);
        assert_eq!(l.input(Key::Other, false), Outcome::Changed);
        assert_eq!(l.menu(), Some(Menu::Navigation));
        assert_eq!(l.input(Key::Backspace, false), Outcome::Changed);
        assert_eq!(l.menu(), Some(Menu::Root));
        assert_eq!(l.input(Key::Leader, false), Outcome::Pass);
        assert_eq!(l.menu(), None);
        l.input(Key::Leader, false);
        assert_eq!(l.input(Key::Escape, false), Outcome::Cancelled);
        assert_eq!(l.menu(), None);
    }
    #[test]
    fn repeats_do_not_execute_and_zoom_alias_works() {
        let mut l = Leader::default();
        l.input(Key::Leader, false);
        l.input(Key::Leader, true);
        l.input(Key::Character('r'), true);
        assert_eq!(l.menu(), Some(Menu::Root));
        l.input(Key::Character('z'), false);
        assert_eq!(
            l.input(Key::Character('='), false),
            Outcome::Execute(Action::ZoomIn)
        );
    }
}
