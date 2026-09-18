use crate::{command::Command, config::Keys};
#[derive(Clone, Debug)]
pub struct Binding {
    pub key: u32,
    pub modifiers: u8,
    pub command: Command,
}
pub const ALT: u8 = 1;
pub const CTRL: u8 = 2;
pub const SHIFT: u8 = 4;
pub const SUPER: u8 = 8;
const NAMED: [(&str, u32); 20] = [
    ("Space", 32),
    ("Enter", 13),
    ("Left", 37),
    ("Up", 38),
    ("Right", 39),
    ("Down", 40),
    ("Escape", 27),
    ("Tab", 9),
    ("F1", 0x70),
    ("F2", 0x71),
    ("F3", 0x72),
    ("F4", 0x73),
    ("F5", 0x74),
    ("F6", 0x75),
    ("F7", 0x76),
    ("F8", 0x77),
    ("F9", 0x78),
    ("F10", 0x79),
    ("F11", 0x7a),
    ("F12", 0x7b),
];
/// Left/right Shift, Ctrl, Alt and the Windows keys.
pub fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0xa0..=0xa5 | 0x5b | 0x5c)
}
/// Virtual key of a printable punctuation character on the active keyboard
/// layout, so `"Alt+Shift+?"` means the physical key that types `?` here.
#[cfg(windows)]
fn layout_key(c: char) -> Option<u32> {
    let code =
        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW(u16::try_from(c).ok()?) };
    (code != -1).then(|| u32::from(code as u16 & 0xff))
}
#[cfg(windows)]
fn layout_char(vk: u32) -> Option<char> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};
    let c = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0x7fff;
    char::from_u32(c).filter(|c| !c.is_control() && *c != ' ')
}
/// US layout stand-in: the tests run without a keyboard.
#[cfg(not(windows))]
const US_OEM: [(char, u32); 11] = [
    (';', 0xba),
    ('=', 0xbb),
    (',', 0xbc),
    ('-', 0xbd),
    ('.', 0xbe),
    ('/', 0xbf),
    ('?', 0xbf),
    ('`', 0xc0),
    ('[', 0xdb),
    ('\\', 0xdc),
    (']', 0xdd),
];
#[cfg(not(windows))]
fn layout_key(c: char) -> Option<u32> {
    US_OEM.iter().find(|(k, _)| *k == c).map(|(_, vk)| *vk)
}
#[cfg(not(windows))]
fn layout_char(vk: u32) -> Option<char> {
    US_OEM.iter().find(|(_, k)| *k == vk).map(|(c, _)| *c)
}
/// Splits `"Ctrl+Alt+K"` into its virtual key and modifier mask.
pub fn chord(key: &str) -> Result<(u32, u8), String> {
    let mut modifiers = 0;
    let mut vk = None;
    for part in key.split('+') {
        let lower = part.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "alt" => Some(ALT),
            "ctrl" => Some(CTRL),
            "shift" => Some(SHIFT),
            "super" => Some(SUPER),
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
        let named = NAMED
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&lower))
            .map(|(_, vk)| *vk);
        let mut chars = part.chars();
        vk = Some(match (named, chars.next(), chars.next()) {
            (Some(vk), _, _) => vk,
            (None, Some(c), None) => {
                char_key(c).ok_or_else(|| format!("unsupported key: {key}"))?
            }
            _ => return Err(format!("unsupported key: {key}")),
        });
    }
    let key_code = vk.ok_or_else(|| format!("missing key: {key}"))?;
    Ok((key_code, modifiers))
}
/// Virtual key of a typed character: letters and digits directly, punctuation
/// through the active layout.
pub fn char_key(c: char) -> Option<u32> {
    if c.is_ascii_alphanumeric() {
        Some(c.to_ascii_uppercase() as u32)
    } else if c.is_ascii_punctuation() {
        layout_key(c)
    } else {
        None
    }
}
/// Configuration spelling of a virtual key, or None when it cannot be bound.
pub fn key_name(vk: u32) -> Option<String> {
    if let Some((name, _)) = NAMED.iter().find(|(_, code)| *code == vk) {
        return Some((*name).into());
    }
    if (0x30..=0x39).contains(&vk) || (0x41..=0x5a).contains(&vk) {
        return char::from_u32(vk).map(String::from);
    }
    layout_char(vk)
        .filter(|c| c.is_ascii_punctuation())
        .map(String::from)
}
/// Inverse of [`chord`], in the canonical modifier order.
pub fn format(vk: u32, modifiers: u8) -> Option<String> {
    let name = key_name(vk)?;
    let mut parts = Vec::new();
    for (bit, label) in [
        (CTRL, "Ctrl"),
        (ALT, "Alt"),
        (SHIFT, "Shift"),
        (SUPER, "Super"),
    ] {
        if modifiers & bit != 0 {
            parts.push(label.to_owned());
        }
    }
    parts.push(name);
    Some(parts.join("+"))
}
pub fn parse(keys: &Keys) -> Result<Vec<Binding>, String> {
    let mut result = Vec::new();
    for (key, command) in &keys.keybindings {
        let (key_code, modifiers) = chord(key)?;
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
        assert_eq!(bs.len(), 58);
        assert!(bs.iter().any(|b| b.command == Command::BarHints
            && b.key == b'B' as u32
            && b.modifiers == CTRL | ALT));
        for (key, increase) in [(b'Y', false), (b'U', true)] {
            assert!(
                bs.iter()
                    .any(|b| b.command == Command::BackgroundOpacity(increase)
                        && b.key == key as u32
                        && b.modifiers == CTRL | ALT | SHIFT)
            );
        }
        assert!(
            bs.iter()
                .any(|b| b.command == Command::ThemePicker && b.key == 32 && b.modifiers == 7)
        );
        assert!(
            bs.iter()
                .any(|b| b.command == Command::WallpaperPicker && b.key == 87 && b.modifiers == 7)
        );
        assert!(
            bs.iter()
                .any(|b| b.command == Command::Keybindings && b.modifiers == ALT | SHIFT)
        );
        assert!(
            bs.iter()
                .all(|b| b.modifiers & 1 != 0 || matches!(b.command, Command::App(_)))
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
        for chord in [
            "Alt+Alt+Q",
            "Alt+A+B",
            "Ctrl+Alt+Delete",
            "Alt",
            "Alt+",
            "",
            "Alt+F13",
            "Alt+é",
        ] {
            let keys = Keys {
                keybindings: [(chord.into(), "quit".into())].into(),
            };
            assert!(parse(&keys).is_err(), "{chord}");
        }
    }
    #[test]
    fn function_keys_bind_alone_or_with_modifiers() {
        let keys = Keys {
            keybindings: [
                ("F8".into(), "dictate".into()),
                ("alt+f1".into(), "quit".into()),
            ]
            .into(),
        };
        let bs = parse(&keys).unwrap();
        assert_eq!(
            (bs[0].key, bs[0].modifiers, bs[0].command.clone()),
            (0x77, 0, Command::Dictate)
        );
        assert_eq!((bs[1].key, bs[1].modifiers), (0x70, ALT));
        assert_eq!(format(0x77, 0).unwrap(), "F8");
        assert_eq!(format(0x7b, ALT | SHIFT).unwrap(), "Alt+Shift+F12");
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
    #[test]
    fn format_roundtrips_every_default() {
        let keys: Keys =
            toml::from_str(include_str!("../config/defaults/keybindings.toml")).unwrap();
        for key in keys.keybindings.keys() {
            let (vk, modifiers) = chord(key).unwrap();
            let spelled = format(vk, modifiers).unwrap();
            assert_eq!(
                chord(&spelled).unwrap(),
                (vk, modifiers),
                "{key} -> {spelled}"
            );
        }
        assert_eq!(
            format(0x41, CTRL | ALT | SHIFT | SUPER).unwrap(),
            "Ctrl+Alt+Shift+Super+A"
        );
        assert_eq!(format(13, ALT).unwrap(), "Alt+Enter");
        assert_eq!(format(0x7c, ALT), None);
        assert!(is_modifier(0xa4));
        assert!(!is_modifier(0x41));
    }
}
