use serde::Deserialize;
use std::{io::Read, path::Path};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub font_family: String,
    /// Points, matching WezTerm (converted to DIPs by the renderer).
    pub font_size: f32,
    pub padding: u16,
    pub scrollback: usize,
    pub distribution: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: "JetBrainsMono Nerd Font Mono".into(),
            font_size: 14.0,
            padding: 4,
            scrollback: 2000,
            distribution: "Debian".into(),
        }
    }
}
impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let c: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        if !c.font_size.is_finite() || !(6.0..=72.0).contains(&c.font_size) {
            return Err("font_size must be finite and between 6 and 72 points".into());
        }
        if c.padding > 64 || c.scrollback > 100_000 {
            return Err("padding must be <= 64 and scrollback <= 100000".into());
        }
        for (name, value) in [
            ("font_family", &c.font_family),
            ("distribution", &c.distribution),
        ] {
            if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err(format!("invalid {name}"));
            }
        }
        Ok(c)
    }
    pub fn load(home: &Path) -> Result<Self, String> {
        let path = home.join("terminal.toml");
        let mut text = String::new();
        match std::fs::File::open(&path) {
            Ok(file) => {
                file.take(65537)
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                if text.len() > 65536 {
                    return Err("terminal.toml exceeds 64 KiB".into());
                }
                Self::parse(&text)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_bounds() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        assert_eq!(
            Config::parse("font_size=16\ndistribution='Ubuntu'")
                .unwrap()
                .distribution,
            "Ubuntu"
        );
        for s in [
            "font_size=nan",
            "font_size=5",
            "font_size=73",
            "padding=65",
            "scrollback=100001",
            "distribution=''",
            "font_family=' '",
            "opacity=0.5",
            "command='herdr'",
        ] {
            assert!(Config::parse(s).is_err(), "{s}");
        }
    }
}
