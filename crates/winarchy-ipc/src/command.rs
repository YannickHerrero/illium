use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Workspace(u8),
    Next,
    Recent,
    Close,
    Focus(Direction),
    Move(Direction),
    MoveWorkspace(u8, bool),
    Tile,
    Float,
    Fullscreen,
    Spawn(String),
    /// Resolved launcher target; aliases and UI results converge on this action.
    LaunchTarget {
        target: String,
        shortcut: bool,
    },
    Launcher,
    /// Session menu: hibernate, lock, restart, shut down, quit.
    Meta,
    /// Launch a companion application from `winarchy-apps.exe`: `shot`, `tasks`, `files`.
    App(String),
    Reload,
    Theme(String),
    /// Open the visual theme picker; browsing does not change the active theme.
    ThemePicker,
    /// Browse images of the active theme without applying until confirmation.
    WallpaperPicker,
    WallpaperNext,
    /// None selects the solid theme background.
    Wallpaper(Option<String>),
    Explorer(bool),
    Quit,
    Status,
}
impl FromStr for Command {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        // Preserve spaces in image names, including CLI arguments joined below.
        if let Some(name) = s.trim().strip_prefix("wallpaper set ") {
            let name = name.trim();
            let name = name
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(name);
            if name.is_empty()
                || name.contains(['/', '\\', ':', '"'])
                || name.chars().any(char::is_control)
                || matches!(name, "." | "..")
            {
                return Err("wallpaper must be a plain file name".into());
            }
            return Ok(Self::Wallpaper(Some(name.into())));
        }
        let parts: Vec<_> = s.split_whitespace().collect();
        let ws = |s: &str| {
            s.parse::<u8>()
                .ok()
                .filter(|n| (1..=9).contains(n))
                .ok_or_else(|| "workspace must be 1..9".to_owned())
        };
        let dir = |s| match s {
            "left" => Ok(Direction::Left),
            "down" => Ok(Direction::Down),
            "up" => Ok(Direction::Up),
            "right" => Ok(Direction::Right),
            _ => Err("invalid direction".to_owned()),
        };
        Ok(match parts.as_slice() {
            ["workspace", "next" | "next-active"] => Self::Next,
            ["workspace", "recent"] => Self::Recent,
            ["workspace", n] => Self::Workspace(ws(n)?),
            ["window", "close"] => Self::Close,
            ["window", "focus", d] => Self::Focus(dir(d)?),
            ["window", "move", d] => Self::Move(dir(d)?),
            ["window", "move-workspace", n] => Self::MoveWorkspace(ws(n)?, false),
            ["window", "move-workspace", n, "--follow"] => Self::MoveWorkspace(ws(n)?, true),
            ["window", "set-tiling"] => Self::Tile,
            ["window", "toggle-float"] => Self::Float,
            ["window", "toggle-fullscreen"] => Self::Fullscreen,
            ["spawn", app] => Self::Spawn((*app).into()),
            ["launcher", "toggle"] => Self::Launcher,
            ["meta", "toggle"] => Self::Meta,
            ["app", name] if name.chars().all(|c| c.is_ascii_lowercase()) => {
                Self::App((*name).into())
            }
            ["config", "reload"] => Self::Reload,
            ["wallpaper", "picker"] => Self::WallpaperPicker,
            ["wallpaper", "next"] => Self::WallpaperNext,
            ["wallpaper", "clear"] => Self::Wallpaper(None),
            ["theme", "picker"] => Self::ThemePicker,
            ["theme", "set", name] if !name.contains(['/', '\\', '.']) => {
                Self::Theme((*name).into())
            }
            ["explorer", "start"] => Self::Explorer(true),
            ["explorer", "stop"] => Self::Explorer(false),
            ["quit"] => Self::Quit,
            ["status"] => Self::Status,
            _ => return Err(format!("unknown command: {s}")),
        })
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands() {
        assert_eq!(
            "window move-workspace 9 --follow".parse(),
            Ok(Command::MoveWorkspace(9, true))
        );
        assert_eq!(
            "window focus left".parse(),
            Ok(Command::Focus(Direction::Left))
        );
        assert_eq!("app shot".parse(), Ok(Command::App("shot".into())));
        assert_eq!("theme picker".parse(), Ok(Command::ThemePicker));
        assert!("theme picker extra".parse::<Command>().is_err());
        for s in [
            "workspace 0",
            "workspace 10",
            "window move diagonal",
            "quit now",
            "theme set ../bad",
            "app",
            "app ../x",
            "app Shot",
        ] {
            assert!(s.parse::<Command>().is_err(), "{s}");
        }
    }
    #[test]
    fn wallpapers() {
        assert_eq!("wallpaper picker".parse(), Ok(Command::WallpaperPicker));
        assert!("wallpaper picker extra".parse::<Command>().is_err());
        assert_eq!("wallpaper next".parse(), Ok(Command::WallpaperNext));
        assert_eq!("wallpaper clear".parse(), Ok(Command::Wallpaper(None)));
        for command in [
            "wallpaper set A painting.jpg",
            "wallpaper set \"A painting.jpg\"",
        ] {
            assert_eq!(
                command.parse(),
                Ok(Command::Wallpaper(Some("A painting.jpg".into())))
            );
        }
        for command in [
            "wallpaper set",
            "wallpaper set ../x.jpg",
            "wallpaper set C:\\x.jpg",
            "wallpaper set \"\"",
            "wallpaper next extra",
        ] {
            assert!(command.parse::<Command>().is_err(), "{command}");
        }
    }
    #[test]
    fn protocol() {
        let r: Reply = serde_json::from_str(r#"{"ok":false,"message":"invalid command"}"#).unwrap();
        assert!(!r.ok);
        assert!(serde_json::from_str::<Reply>("{}").is_err());
    }
}
