//! Keyboard vocabulary for the bar's one-key hint mode (Windows virtual keys).
pub const LABELS: &[u8] = b"123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub fn label(index: usize) -> Option<char> {
    LABELS.get(index).copied().map(char::from)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Select(usize),
    Previous,
    Next,
    Accept,
    Cancel,
}

pub fn input(key: u32) -> Option<Input> {
    Some(match key {
        // Virtual digits are independent of Shift: &/é/… work on AZERTY.
        0x31..=0x39 => Input::Select((key - 0x31) as usize),
        0x61..=0x69 => Input::Select((key - 0x61) as usize),
        0x41..=0x5a => Input::Select((key - 0x41) as usize + 9),
        0x25 => Input::Previous,
        0x27 => Input::Next,
        0x0d => Input::Accept,
        0x1b => Input::Cancel,
        _ => return None,
    })
}

pub fn step(selected: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (selected + 1) % count
    } else {
        (selected + count - 1) % count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_keys_agree_without_shift_or_character_translation() {
        for index in 0..LABELS.len() {
            assert_eq!(input(label(index).unwrap() as u32), Some(Input::Select(index)));
        }
        assert_eq!(label(8), Some('9'));
        assert_eq!(label(9), Some('A'));
        assert_eq!(label(34), Some('Z'));
        assert_eq!(label(35), None);
        for key in 0x61..=0x69 {
            assert_eq!(input(key), input(key - 0x30));
        }
        assert_eq!(input(0x30), None);
        assert_eq!(input(0x70), None);
    }

    #[test]
    fn navigation_wraps_and_handles_empty_bars() {
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 0, false), 0);
        assert_eq!(step(0, 0, true), 0);
        assert_eq!(input(0x1b), Some(Input::Cancel));
        assert_eq!(input(0x0d), Some(Input::Accept));
    }
}
