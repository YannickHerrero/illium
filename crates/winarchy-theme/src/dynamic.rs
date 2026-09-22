//! Wallpaper-derived palettes. Consumers need no image/graphics dependency.
use crate::Theme;
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const FILE: &str = "dynamic-theme.state";
pub fn is_dynamic(name: &str) -> bool {
    matches!(name, "dynamic-dark" | "dynamic-light")
}
/// Both variants share one extraction and one wallpaper choice.
pub fn selection_key(name: &str) -> &str {
    if is_dynamic(name) {
        "dynamic-dark"
    } else {
        name
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palettes {
    pub dark: Theme,
    pub light: Theme,
}
impl Palettes {
    pub fn get(&self, name: &str) -> &Theme {
        if name == "dynamic-light" {
            &self.light
        } else {
            &self.dark
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.dark.validate()?;
        self.light.validate()?;
        if self.dark.mode.as_deref() != Some("dark") || self.light.mode.as_deref() != Some("light")
        {
            return Err("invalid dynamic palette modes".into());
        }
        Ok(())
    }
}

mod state;
pub use state::{Snapshot, apply, publish};
#[cfg(feature = "assets")]
mod cache;
#[cfg(feature = "assets")]
pub use cache::prepare;

#[cfg(feature = "assets")]
mod generate;
#[cfg(feature = "assets")]
pub use generate::generate;
