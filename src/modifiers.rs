//! Modifier state follows the ordered keyboard event stream, not an asynchronous
//! OS snapshot queried from inside a low-level keyboard callback.
#[derive(Default)]
pub struct Modifiers {
    pressed: u8,
}
impl Modifiers {
    pub const fn new() -> Self {
        Self { pressed: 0 }
    }
    pub fn update(&mut self, key: u32, down: bool) {
        let bit = match key {
            0x10 | 0xa0 => 1,
            0xa1 => 2,
            0x11 | 0xa2 => 4,
            0xa3 => 8,
            0x12 | 0xa4 => 16,
            0xa5 => 32,
            0x5b => 64,
            0x5c => 128,
            _ => return,
        };
        if down {
            self.pressed |= bit;
        } else {
            self.pressed &= !bit;
        }
    }
    pub fn mask(&self) -> u8 {
        u8::from(self.pressed & 48 != 0)
            | (u8::from(self.pressed & 12 != 0) * 2)
            | (u8::from(self.pressed & 3 != 0) * 4)
            | (u8::from(self.pressed & 192 != 0) * 8)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_chords_follow_event_order() {
        let mut m = Modifiers::new();
        m.update(0x12, true);
        assert_eq!(m.mask(), 1);
        m.update(0x10, true);
        m.update(0x48, true);
        assert_eq!(m.mask(), 5);
        m.update(0x48, false);
        assert_eq!(m.mask(), 5);
        m.update(0x10, false);
        assert_eq!(m.mask(), 1);
        m.update(0x12, false);
        assert_eq!(m.mask(), 0);
    }
    #[test]
    fn releasing_one_side_does_not_clear_the_other() {
        let mut m = Modifiers::new();
        m.update(0xa0, true);
        m.update(0xa1, true);
        m.update(0xa0, false);
        assert_eq!(m.mask(), 4);
        m.update(0xa1, false);
        assert_eq!(m.mask(), 0);
    }
    #[test]
    fn altgr_does_not_match_plain_alt() {
        let mut m = Modifiers::new();
        m.update(0xa2, true);
        m.update(0xa5, true);
        assert_eq!(m.mask(), 3);
        m.update(0xa5, false);
        m.update(0xa2, false);
        assert_eq!(m.mask(), 0);
    }
}
