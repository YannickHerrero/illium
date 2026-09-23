//! Winarchy's own lock screen: the password file, attempt counting and the
//! keys withheld from the lock surface. Windowing lives in the platform layer.
use crate::keyboard::{ALT, CTRL, SUPER};
use argon2::{Argon2, password_hash::PasswordVerifier};
use std::path::{Path, PathBuf};

/// Failed attempts before Winarchy hands over to the Windows lock.
pub const ATTEMPTS: u8 = 5;
/// `winarchyctl lock set-password` writes the same file name.
pub fn path(home: &Path) -> PathBuf {
    home.join("lock-password")
}
/// Argon2 PHC string of the lock password.
pub fn load(home: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path(home)).map_err(|e| e.to_string())?;
    let hash = text.trim();
    argon2::password_hash::phc::PasswordHash::new(hash).map_err(|e| e.to_string())?;
    Ok(hash.to_owned())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Unlock,
    Wrong { remaining: u8 },
    Exhausted,
}
#[derive(Default)]
pub struct Attempts {
    failed: u8,
}
impl Attempts {
    pub fn check(&mut self, hash: &str, password: &str) -> Verdict {
        if Argon2::default()
            .verify_password(password.as_bytes(), hash)
            .is_ok()
        {
            self.failed = 0;
            return Verdict::Unlock;
        }
        self.failed = self.failed.saturating_add(1);
        if self.failed >= ATTEMPTS {
            Verdict::Exhausted
        } else {
            Verdict::Wrong {
                remaining: ATTEMPTS - self.failed,
            }
        }
    }
}
/// Whether the keyboard hook swallows a key press while locked. Ctrl+Alt
/// passes because it is how Windows reports AltGr, which types characters
/// such as `@` or `€` on many layouts. Ctrl+Alt+Del cannot be intercepted.
pub fn blocked(vk: u32, modifiers: u8) -> bool {
    const TAB: u32 = 0x09;
    const ESCAPE: u32 = 0x1b;
    if matches!(vk, 0x5b | 0x5c) || modifiers & SUPER != 0 {
        return true;
    }
    if crate::keyboard::is_modifier(vk) {
        return false;
    }
    let (ctrl, alt) = (modifiers & CTRL != 0, modifiers & ALT != 0);
    ctrl != alt || (alt && matches!(vk, TAB | ESCAPE))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::SHIFT;
    use argon2::password_hash::PasswordHasher;
    fn hash(password: &str) -> String {
        Argon2::default()
            .hash_password(password.as_bytes())
            .unwrap()
            .to_string()
    }
    #[test]
    fn five_failures_exhaust_and_success_resets() {
        let hash = hash("correct horse");
        let mut attempts = Attempts::default();
        assert_eq!(
            attempts.check(&hash, "wrong"),
            Verdict::Wrong { remaining: 4 }
        );
        assert_eq!(attempts.check(&hash, "correct horse"), Verdict::Unlock);
        for remaining in (1..ATTEMPTS).rev() {
            assert_eq!(attempts.check(&hash, ""), Verdict::Wrong { remaining });
        }
        assert_eq!(attempts.check(&hash, "Correct horse"), Verdict::Exhausted);
    }
    #[test]
    fn load_rejects_missing_and_malformed_files() {
        let home = std::env::temp_dir().join(format!("winarchy-lock-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        assert!(load(&home).is_err());
        std::fs::write(path(&home), "not a hash").unwrap();
        assert!(load(&home).is_err());
        let hash = hash("pw");
        std::fs::write(path(&home), format!("{hash}\r\n")).unwrap();
        assert_eq!(load(&home), Ok(hash));
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn navigation_is_withheld_but_typing_passes() {
        let l = 0x4c;
        for (vk, modifiers) in [
            (0x5b, 0),
            (l, SUPER),
            (0x09, ALT),
            (0x09, CTRL | ALT),
            (0x1b, CTRL),
            (0x1b, CTRL | SHIFT),
            (0x73, ALT),
            (l, CTRL | SHIFT),
        ] {
            assert!(blocked(vk, modifiers), "{vk:#x} {modifiers}");
        }
        for (vk, modifiers) in [
            (l, 0),
            (l, SHIFT),
            (0x30, CTRL | ALT),
            (0x0d, 0),
            (0x08, 0),
            (0xa2, CTRL),
            (0xa5, CTRL | ALT),
            (0x10, SHIFT),
        ] {
            assert!(!blocked(vk, modifiers), "{vk:#x} {modifiers}");
        }
    }
}
