//! Illium Mono: JetBrains Mono plus the Japanese glyphs of Noto Sans JP, built
//! by `scripts/build-illium-mono.py`. Slint's software renderer has no glyph
//! fallback and no API for the default font, but reads `SLINT_DEFAULT_FONT`
//! before its first use: the files go under `%LOCALAPPDATA%\Illium\fonts` and
//! that variable points there, so every text without an explicit family uses
//! Illium Mono, applet views included.
use std::path::{Path, PathBuf};

pub const FAMILY: &str = "Illium Mono";
const FILES: [(&str, &[u8]); 2] = [
    (
        "IlliumMono-Regular.ttf",
        include_bytes!("../../../ui/fonts/IlliumMono-Regular.ttf"),
    ),
    (
        "IlliumMono-Bold.ttf",
        include_bytes!("../../../ui/fonts/IlliumMono-Bold.ttf"),
    ),
];

/// Writes the fonts and points Slint at them. Call before the first Slint use
/// and before spawning threads; a `SLINT_DEFAULT_FONT` already set wins.
pub fn install() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let dir = Path::new(&local).join("Illium").join("fonts");
    write(&dir)?;
    if std::env::var_os("SLINT_DEFAULT_FONT").is_none() {
        // SAFETY: called from the main thread before any other thread exists.
        unsafe { std::env::set_var("SLINT_DEFAULT_FONT", &dir) };
    }
    Ok(dir)
}
/// Writes the font files into `dir`, leaving up-to-date ones untouched.
pub fn write(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, bytes) in FILES {
        let path = dir.join(name);
        if std::fs::read(&path).ok().as_deref() != Some(bytes) {
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_both_weights_once() {
        let dir = std::env::temp_dir().join(format!("illium-fonts-{}", std::process::id()));
        write(&dir).unwrap();
        let regular = dir.join("IlliumMono-Regular.ttf");
        let written = std::fs::metadata(&regular).unwrap().modified().unwrap();
        assert_eq!(std::fs::read(&regular).unwrap(), FILES[0].1);
        assert_eq!(
            std::fs::read(dir.join("IlliumMono-Bold.ttf")).unwrap(),
            FILES[1].1
        );
        write(&dir).unwrap();
        assert_eq!(
            std::fs::metadata(&regular).unwrap().modified().unwrap(),
            written
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
