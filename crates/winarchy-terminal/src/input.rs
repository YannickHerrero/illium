//! Small legacy/xterm input encoder. CSI-u Ctrl+digits is intentional; do not
//! advertise full Kitty keyboard support until its negotiation is implemented.
use alacritty_terminal::term::TermMode;
#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}
impl Mods {
    fn parameter(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }
}
/// Windows virtual keys; text/dead keys/AltGr are handled by WM_CHAR instead.
pub fn key(vk: u16, m: Mods, mode: TermMode) -> Option<Vec<u8>> {
    if m.ctrl && (0x31..=0x39).contains(&vk) {
        return Some(format!("\x1b[{vk};{}u", m.parameter()).into_bytes());
    }
    let p = m.parameter();
    let arrow = match vk {
        0x25 => Some('D'),
        0x26 => Some('A'),
        0x27 => Some('C'),
        0x28 => Some('B'),
        0x24 => Some('H'),
        0x23 => Some('F'),
        _ => None,
    };
    if let Some(c) = arrow {
        return Some(
            if p != 1 {
                format!("\x1b[1;{p}{c}")
            } else if mode.contains(TermMode::APP_CURSOR) {
                format!("\x1bO{c}")
            } else {
                format!("\x1b[{c}")
            }
            .into_bytes(),
        );
    }
    let tilde = match vk {
        0x2d => Some(2),
        0x2e => Some(3),
        0x21 => Some(5),
        0x22 => Some(6),
        0x74 => Some(15),
        0x75 => Some(17),
        0x76 => Some(18),
        0x77 => Some(19),
        0x78 => Some(20),
        0x79 => Some(21),
        0x7a => Some(23),
        0x7b => Some(24),
        _ => None,
    };
    if let Some(n) = tilde {
        return Some(
            if p == 1 {
                format!("\x1b[{n}~")
            } else {
                format!("\x1b[{n};{p}~")
            }
            .into_bytes(),
        );
    }
    if (0x70..=0x73).contains(&vk) {
        let c = (b'P' + (vk - 0x70) as u8) as char;
        return Some(
            if p == 1 {
                format!("\x1bO{c}")
            } else {
                format!("\x1b[1;{p}{c}")
            }
            .into_bytes(),
        );
    }
    if vk == 9 && m.shift {
        return Some(b"\x1b[Z".to_vec());
    }
    if vk == 8 {
        return Some(if m.alt { vec![27, 127] } else { vec![127] });
    }
    None
}
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Never allow clipboard ESC to terminate bracketed paste and inject keys.
    let text = text
        .replace('\x1b', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.replace('\n', "\r").into_bytes()
    }
}
pub fn mouse(
    button: u8,
    col: usize,
    row: usize,
    release: bool,
    mods: Mods,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let code =
        button + 4 * u8::from(mods.shift) + 8 * u8::from(mods.alt) + 16 * u8::from(mods.ctrl);
    if mode.contains(TermMode::SGR_MOUSE) {
        Some(
            format!(
                "\x1b[<{code};{};{}{}",
                col + 1,
                row + 1,
                if release { 'm' } else { 'M' }
            )
            .into_bytes(),
        )
    } else if col < 223 && row < 223 {
        Some(vec![
            27,
            b'[',
            b'M',
            32 + if release { 3 } else { code },
            33 + col as u8,
            33 + row as u8,
        ])
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn herdr_and_navigation() {
        for digit in 1..=9 {
            assert_eq!(
                key(
                    48 + digit,
                    Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                    TermMode::empty()
                )
                .unwrap(),
                format!("\x1b[{};5u", 48 + digit).as_bytes()
            );
        }
        assert_eq!(
            key(0x26, Mods::default(), TermMode::APP_CURSOR).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(
            key(
                0x26,
                Mods {
                    ctrl: true,
                    ..Mods::default()
                },
                TermMode::empty()
            )
            .unwrap(),
            b"\x1b[1;5A"
        );
        assert!(key(0x41, Mods::default(), TermMode::empty()).is_none());
    }
    #[test]
    fn paste_and_mouse() {
        assert_eq!(
            paste("a\r\nb\x1b[201~", true),
            b"\x1b[200~a\nb[201~\x1b[201~"
        );
        assert_eq!(
            mouse(0, 2, 3, true, Mods::default(), TermMode::SGR_MOUSE).unwrap(),
            b"\x1b[<0;3;4m"
        );
        assert!(mouse(0, 223, 0, false, Mods::default(), TermMode::empty()).is_none());
    }
}
