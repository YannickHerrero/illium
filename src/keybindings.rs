//! Keybindings editor model: what the list shows and how the file is rewritten.
//! Windowing and key capture live in the platform layer.
use crate::{
    command::{Command, Direction},
    keyboard,
};
use toml_edit::{DocumentMut, Item, Table, Value};
pub const DEFAULTS: &str = include_str!("../config/defaults/keybindings.toml");
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub command: String,
    pub description: String,
    /// Chord as spelled in the user's file; None for an unbound default.
    pub chord: Option<String>,
    /// Chord shipped for this command; None for a user addition.
    pub default: Option<String>,
}
impl Row {
    pub fn changed(&self) -> bool {
        match (&self.chord, &self.default) {
            (Some(chord), Some(default)) => !same(chord, default),
            (None, Some(_)) => true,
            (_, None) => false,
        }
    }
    pub fn label(&self) -> String {
        self.chord
            .as_deref()
            .map(pretty)
            .unwrap_or_else(|| "Unbound".into())
    }
}
/// `"alt+shift+k"` shown as `"Alt + Shift + K"`. A punctuation key keeps the
/// user's spelling: `?` stays `?` even where the layout maps it to the comma key.
pub fn pretty(chord: &str) -> String {
    let Ok((vk, modifiers)) = keyboard::chord(chord) else {
        return chord.to_owned();
    };
    let Some(formatted) = keyboard::format(vk, modifiers) else {
        return chord.to_owned();
    };
    let mut parts: Vec<&str> = formatted.split('+').collect();
    if let Some(typed) = chord.split('+').find(|part| {
        part.chars()
            .next()
            .is_some_and(|c| c.is_ascii_punctuation())
    }) && let Some(last) = parts.last_mut()
    {
        *last = typed;
    }
    parts.join(" + ")
}
fn same(a: &str, b: &str) -> bool {
    match (keyboard::chord(a), keyboard::chord(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
fn direction(d: Direction) -> &'static str {
    match d {
        Direction::Left => "left",
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::Right => "right",
    }
}
pub fn describe(command: &str) -> String {
    let Ok(parsed) = command.parse::<Command>() else {
        return command.to_owned();
    };
    match parsed {
        Command::Workspace(n) => format!("Workspace {n}"),
        Command::Next => "Next occupied workspace".into(),
        Command::Recent => "Recent workspace".into(),
        Command::Close => "Close window".into(),
        Command::Focus(d) => format!("Focus {}", direction(d)),
        Command::Move(d) => format!("Move window {}", direction(d)),
        Command::MoveWorkspace(n, true) => format!("Move window to workspace {n} and follow"),
        Command::MoveWorkspace(n, false) => format!("Move window to workspace {n}"),
        Command::Tile => "Set tiling".into(),
        Command::Float => "Toggle floating".into(),
        Command::Fullscreen => "Toggle fullscreen".into(),
        Command::Spawn(app) if app == "terminal" => "Terminal".into(),
        Command::Spawn(app) => format!("Launch {app}"),
        Command::LaunchTarget { target, .. } => format!("Launch {target}"),
        Command::Launcher => "Launcher".into(),
        Command::Meta => "Menu".into(),
        Command::App(name) => match name.as_str() {
            "files" => "File manager".into(),
            "tasks" => "Task manager".into(),
            "shot" => "Screenshot region to clipboard".into(),
            other => format!("Application {other}"),
        },
        Command::Reload => "Reload configuration".into(),
        Command::Theme(name) => format!("Theme {name}"),
        Command::BackgroundOpacity(true) => "Increase background opacity".into(),
        Command::BackgroundOpacity(false) => "Decrease background opacity".into(),
        Command::ThemePicker => "Theme picker".into(),
        Command::WallpaperPicker => "Wallpaper picker".into(),
        Command::WallpaperNext => "Next wallpaper".into(),
        Command::Keybindings => "Keybindings".into(),
        Command::Dictate => "Dictate while held".into(),
        Command::Wallpaper(None) => "Solid background".into(),
        Command::Wallpaper(Some(name)) => format!("Wallpaper {name}"),
        Command::Explorer(true) => "Start Explorer".into(),
        Command::Explorer(false) => "Stop Explorer".into(),
        Command::Quit => "Quit Winarchy".into(),
        Command::Status => "Status".into(),
    }
}
/// `(chord, command)` pairs in file order; invalid documents yield nothing.
fn entries(text: &str) -> Vec<(String, String)> {
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return vec![];
    };
    doc.get("keybindings")
        .and_then(Item::as_table)
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, value)| Some((key.to_owned(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}
/// Defaults in shipped order, each paired with the user's chord for the same
/// command; a command bound several times pairs identical chords first, then
/// the remaining ones in order. Extra user bindings follow the defaults.
pub fn rows(defaults: &str, user: &str) -> Vec<Row> {
    let defaults = entries(defaults);
    let mut user: Vec<Option<(String, String)>> = entries(user).into_iter().map(Some).collect();
    let mut take = |command: &str, exact: Option<&str>| -> Option<String> {
        let index = user.iter().position(|entry| {
            entry.as_ref().is_some_and(|(chord, c)| {
                c == command && exact.is_none_or(|exact| same(chord, exact))
            })
        })?;
        user[index].take().map(|(chord, _)| chord)
    };
    let mut found: Vec<Option<String>> = defaults
        .iter()
        .map(|(chord, command)| take(command, Some(chord)))
        .collect();
    for (index, (_, command)) in defaults.iter().enumerate() {
        if found[index].is_none() {
            found[index] = take(command, None);
        }
    }
    let mut result: Vec<Row> = defaults
        .into_iter()
        .zip(found)
        .map(|((default, command), chord)| Row {
            description: describe(&command),
            command,
            chord,
            default: Some(default),
        })
        .collect();
    result.extend(user.into_iter().flatten().map(|(chord, command)| Row {
        description: describe(&command),
        command,
        chord: Some(chord),
        default: None,
    }));
    result
}
/// Case-insensitive substring match on the description, the chord or the
/// command; spaces and `+` are ignored so `alt shift 3` finds `Alt+Shift+3`.
pub fn filter(rows: &[Row], query: &str) -> Vec<usize> {
    let query = normalize(query);
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            query.is_empty()
                || [row.description.as_str(), &row.label(), &row.command]
                    .iter()
                    .any(|text| normalize(text).contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '+')
        .flat_map(char::to_lowercase)
        .collect()
}
/// Index of another row holding `chord`.
pub fn conflict(rows: &[Row], chord: &str, except: usize) -> Option<usize> {
    rows.iter()
        .position(|row| row.chord.as_deref().is_some_and(|c| same(c, chord)))
        .filter(|index| *index != except)
}
/// Drops the `remove` chords and binds `chord` to `command` where the first
/// removed key stood, keeping every other line, comment and order intact.
pub fn rewrite(text: &str, remove: &[&str], chord: &str, command: &str) -> Result<String, String> {
    let mut doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let table = doc
        .entry("keybindings")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or("keybindings is not a table")?;
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    let removed: Vec<&String> = keys
        .iter()
        .filter(|k| remove.iter().any(|r| same(k, r)) || same(k, chord))
        .collect();
    let mut kept = Vec::new();
    let mut inserted = false;
    let new_key = toml_edit::Key::new(chord);
    let new_item = Item::Value(Value::from(command));
    for key in &keys {
        let Some((k, item)) = table.remove_entry(key) else {
            continue;
        };
        if removed.contains(&key) {
            if !inserted {
                kept.push((
                    new_key.clone().with_leaf_decor(k.leaf_decor().clone()),
                    new_item.clone(),
                ));
                inserted = true;
            }
            continue;
        }
        kept.push((k, item));
    }
    if !inserted {
        kept.push((new_key, new_item));
    }
    for (key, item) in kept {
        table.insert_formatted(&key, item);
    }
    Ok(doc.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_unchanged_and_ordered() {
        let rows = rows(DEFAULTS, DEFAULTS);
        assert_eq!(rows.len(), 53);
        assert!(rows.iter().all(|r| !r.changed()));
        assert_eq!(rows[0].description, "Launcher");
        assert_eq!(rows[0].label(), "Alt + Space");
        assert_eq!(rows[2].command, "keybindings toggle");
        assert_eq!(rows[2].label(), "Alt + Shift + ?");
        assert_eq!(pretty("shift+alt+q"), "Alt + Shift + Q");
        assert!(
            rows.iter()
                .any(|r| r.description == "Move window to workspace 3 and follow")
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.description == "Focus left")
                .count(),
            2
        );
    }
    #[test]
    fn rebound_unbound_and_custom() {
        let user = "[keybindings]\n\"Ctrl+Alt+Space\" = \"launcher toggle\"\n\"Alt+Left\" = \"window focus left\"\n\"Alt+Shift+H\" = \"window move left\"\n\"alt+q\" = \"window close\"\n\"Alt+Z\" = \"spawn editor\"\n";
        let rows = rows(DEFAULTS, user);
        let by = |d: &str| {
            rows.iter()
                .filter(|r| r.description == d)
                .collect::<Vec<_>>()
        };
        let launcher = &by("Launcher")[0];
        assert_eq!(launcher.chord.as_deref(), Some("Ctrl+Alt+Space"));
        assert!(launcher.changed());
        let focus = by("Focus left");
        assert_eq!(focus[0].default.as_deref(), Some("Alt+H"));
        assert_eq!(focus[0].chord, None);
        assert!(focus[0].changed());
        assert_eq!(focus[1].chord.as_deref(), Some("Alt+Left"));
        assert!(!focus[1].changed());
        assert!(!by("Close window")[0].changed());
        assert!(!by("Move window left")[0].changed());
        let custom = rows.last().unwrap();
        assert_eq!(custom.description, "Launch editor");
        assert_eq!(custom.default, None);
        assert!(!custom.changed());
    }
    #[test]
    fn filtering_and_conflicts() {
        let rows = rows(DEFAULTS, DEFAULTS);
        let hits = filter(&rows, "screen");
        assert!(hits.iter().all(|i| {
            let r = &rows[*i];
            r.description.to_lowercase().contains("screen")
        }));
        assert!(hits.len() >= 2);
        assert_eq!(filter(&rows, "").len(), rows.len());
        assert!(
            filter(&rows, "alt shift 3").contains(
                &rows
                    .iter()
                    .position(|r| r.chord.as_deref() == Some("Alt+Shift+3"))
                    .unwrap()
            )
        );
        let launcher = 0;
        let meta = rows
            .iter()
            .position(|r| r.command == "meta toggle")
            .unwrap();
        assert_eq!(conflict(&rows, "alt+shift+space", launcher), Some(meta));
        assert_eq!(conflict(&rows, "Alt+Space", launcher), None);
        assert_eq!(conflict(&rows, "Ctrl+Alt+Z", launcher), None);
    }
    #[test]
    fn rewrite_keeps_comments_and_position() {
        let text = "# mine\n[keybindings]\n# launcher\n\"Alt+Space\" = \"launcher toggle\" # trailing\n\"Alt+Q\" = \"window close\"\n";
        let out = rewrite(text, &["Alt+Space"], "Ctrl+Space", "launcher toggle").unwrap();
        assert_eq!(
            out,
            "# mine\n[keybindings]\n# launcher\n\"Ctrl+Space\" = \"launcher toggle\"\n\"Alt+Q\" = \"window close\"\n"
        );
        // Replacing a conflict removes the other command's line.
        let out = rewrite(text, &["Alt+Space", "Alt+Q"], "Alt+Q", "launcher toggle").unwrap();
        assert_eq!(
            out,
            "# mine\n[keybindings]\n# launcher\n\"Alt+Q\" = \"launcher toggle\"\n"
        );
        // An unbound default gets appended.
        let out = rewrite(text, &[], "Alt+F", "window toggle-fullscreen").unwrap();
        assert!(
            out.ends_with(
                "\"Alt+Q\" = \"window close\"\n\"Alt+F\" = \"window toggle-fullscreen\"\n"
            )
        );
        let keys: crate::config::Keys = toml::from_str(&out).unwrap();
        assert_eq!(keys.keybindings.len(), 3);
        assert!(rewrite("[keybindings\n", &[], "Alt+F", "quit").is_err());
    }
}
