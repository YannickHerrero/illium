use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Width,
    Height,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Workspace(u8),
    Next,
    Recent,
    Close,
    Focus(Direction),
    Move(Direction),
    /// Change a tiled split by signed percentage points of its available space.
    Resize {
        axis: Axis,
        delta: i32,
    },
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
    /// Adjust shared application opacity by five points (true increases).
    BackgroundOpacity(bool),
    /// Open the visual theme picker; browsing does not change the active theme.
    ThemePicker,
    /// Browse images of the active theme without applying until confirmation.
    WallpaperPicker,
    WallpaperNext,
    /// Open the keybindings viewer and editor.
    Keybindings,
    /// Push-to-talk dictation through `winarchy-dictate.exe`: bound to a held
    /// key, the daemon records while it is down; over IPC it toggles.
    Dictate,
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
            ["window", "resize", axis, amount] => {
                let axis = match *axis {
                    "--width" => Axis::Width,
                    "--height" => Axis::Height,
                    _ => return Err("resize axis must be --width or --height".into()),
                };
                let delta = amount.strip_suffix('%')
                    .filter(|s| s.starts_with(['+', '-']))
                    .and_then(|s| s.parse::<i32>().ok())
                    .filter(|n| *n != 0 && (-100..=100).contains(n))
                    .ok_or("resize requires a signed integer percentage from -100% to +100%, excluding zero")?;
                Self::Resize { axis, delta }
            }
            ["window", "move-workspace", n] => Self::MoveWorkspace(ws(n)?, false),
            ["window", "move-workspace", n, "--follow"] => Self::MoveWorkspace(ws(n)?, true),
            ["window", "set-tiling"] => Self::Tile,
            ["window", "toggle-float"] => Self::Float,
            ["window", "toggle-fullscreen"] => Self::Fullscreen,
            ["spawn", app] => Self::Spawn((*app).into()),
            ["launcher", "toggle"] => Self::Launcher,
            ["meta", "toggle"] => Self::Meta,
            ["keybindings", "toggle"] => Self::Keybindings,
            ["dictate"] => Self::Dictate,
            ["app", name] if name.chars().all(|c| c.is_ascii_lowercase()) => {
                Self::App((*name).into())
            }
            ["config", "reload"] => Self::Reload,
            ["wallpaper", "picker"] => Self::WallpaperPicker,
            ["wallpaper", "next"] => Self::WallpaperNext,
            ["wallpaper", "clear"] => Self::Wallpaper(None),
            ["theme", "picker"] => Self::ThemePicker,
            ["opacity", "increase"] => Self::BackgroundOpacity(true),
            ["opacity", "decrease"] => Self::BackgroundOpacity(false),
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
        assert_eq!(
            "opacity increase".parse(),
            Ok(Command::BackgroundOpacity(true))
        );
        assert_eq!(
            "opacity decrease".parse(),
            Ok(Command::BackgroundOpacity(false))
        );
        assert!("opacity increase extra".parse::<Command>().is_err());
        assert!("opacity set nan".parse::<Command>().is_err());
        assert_eq!("keybindings toggle".parse(), Ok(Command::Keybindings));
        assert_eq!("dictate".parse(), Ok(Command::Dictate));
        assert!("dictate toggle".parse::<Command>().is_err());
        assert!("keybindings".parse::<Command>().is_err());
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
    fn resize() {
        for (flag, axis) in [("--width", Axis::Width), ("--height", Axis::Height)] {
            for delta in [-100, -5, 5, 100] {
                let command = Command::Resize { axis, delta };
                assert_eq!(
                    format!("window resize {flag} {delta:+}%").parse(),
                    Ok(command.clone())
                );
                let json = serde_json::to_string(&command).unwrap();
                assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            }
        }
        for s in [
            "",
            "--width",
            "--depth +5%",
            "--width 5%",
            "--width +0%",
            "--width +101%",
            "--height -101%",
            "--width +5",
            "--width +1.5%",
            "--width +5% --height +5%",
            "--width +999999999999%",
            "--width NaN",
        ] {
            assert!(
                format!("window resize {s}").parse::<Command>().is_err(),
                "{s}"
            );
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
