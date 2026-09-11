use crate::{command::Command, config::Keys};
#[derive(Clone, Debug)]
pub struct Binding {
    pub key: u32,
    pub modifiers: u8,
    pub command: Command,
}
pub fn parse(keys: &Keys) -> Result<Vec<Binding>, String> {
    let mut result = Vec::new();
    for (key, command) in &keys.keybindings {
        let mut modifiers = 0;
        let mut vk = None;
        for part in key.split('+') {
            let part = part.to_ascii_lowercase();
            let modifier = match part.as_str() {
                "alt" => Some(1),
                "ctrl" => Some(2),
                "shift" => Some(4),
                "super" => Some(8),
                _ => None,
            };
            if let Some(m) = modifier {
                if modifiers & m != 0 {
                    return Err(format!("duplicate modifier: {key}"));
                }
                modifiers |= m;
                continue;
            }
            if vk.is_some() {
                return Err(format!("multiple keys in chord: {key}"));
            }
            vk = Some(match part.as_str() {
                "space" => 32,
                "enter" => 13,
                "left" => 37,
                "up" => 38,
                "right" => 39,
                "down" => 40,
                "escape" => 27,
                "tab" => 9,
                s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => {
                    s.to_ascii_uppercase().as_bytes()[0] as u32
                }
                _ => return Err(format!("unsupported key: {key}")),
            });
        }
        let key_code = vk.ok_or_else(|| format!("missing key: {key}"))?;
        if result
            .iter()
            .any(|b: &Binding| b.key == key_code && b.modifiers == modifiers)
        {
            return Err(format!("duplicate chord: {key}"));
        }
        result.push(Binding {
            key: key_code,
            modifiers,
            command: command.parse()?,
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_defaults() {
        let keys: Keys =
            toml::from_str(include_str!("../config/defaults/keybindings.toml")).unwrap();
        let bs = parse(&keys).unwrap();
        assert_eq!(bs.len(), 45);
        assert!(
            bs.iter()
                .all(|b| b.modifiers & 1 != 0 || b.command == Command::Screenshot)
        );
        assert_eq!(
            bs.iter()
                .find(|b| b.key == 32 && b.modifiers == 1)
                .unwrap()
                .command,
            Command::Launcher
        );
    }
    #[test]
    fn reject_ambiguous_chords() {
        for chord in ["Alt+Alt+Q", "Alt+A+B", "Ctrl+Alt+Delete", "Alt", "Alt+", ""] {
            let keys = Keys {
                keybindings: [(chord.into(), "quit".into())].into(),
            };
            assert!(parse(&keys).is_err(), "{chord}");
        }
    }
    #[test]
    fn aliases_and_modifiers() {
        let keys = Keys {
            keybindings: [("Ctrl+Super+Shift+Q".into(), "quit".into())].into(),
        };
        assert_eq!(parse(&keys).unwrap()[0].modifiers, 14);
        let keys = Keys {
            keybindings: [
                ("Alt+Q".into(), "quit".into()),
                ("alt+q".into(), "quit".into()),
            ]
            .into(),
        };
        assert!(parse(&keys).is_err());
    }
}
