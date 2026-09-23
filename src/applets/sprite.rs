//! Data-only sprite packs: no pet names or atlas conventions in the renderer.
use super::icon_file;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    pub sheet: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub interval_ms: u32,
    pub display_height: u32,
    /// Provider template, e.g. `{pet_state}`. Unknown values use `idle`.
    pub state: String,
    pub states: BTreeMap<String, Animation>,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub row: u32,
    pub frames: u32,
}
impl Sprite {
    pub fn parse(text: &str) -> Result<Self, String> {
        let sprite: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        if icon_file(&sprite.sheet, &serde_json::Value::Null).as_deref() != Some(&sprite.sheet)
            || !(1..=512).contains(&sprite.frame_width)
            || !(1..=512).contains(&sprite.frame_height)
            || !(1..=32).contains(&sprite.columns)
            || !(1..=32).contains(&sprite.rows)
            || !(60..=2000).contains(&sprite.interval_ms)
            || !(16..=48).contains(&sprite.display_height)
            || sprite.frame_width * sprite.columns > 4096
            || sprite.frame_height * sprite.rows > 4096
            || !sprite.states.contains_key("idle")
            || sprite.states.values().any(|a| a.row >= sprite.rows || a.frames == 0 || a.frames > sprite.columns)
        {
            return Err("invalid sprite dimensions, cadence, sheet or animations".into());
        }
        Ok(sprite)
    }
    pub fn animation(&self, data: &serde_json::Value) -> Animation {
        let state = super::label(&self.state, data);
        self.states.get(&state).copied().unwrap_or(self.states["idle"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PACK: &str = r#"
sheet = "cat.png"
frame_width = 192
frame_height = 208
columns = 8
rows = 9
interval_ms = 140
display_height = 28
state = "{pet_state}"
[states]
idle = { row = 0, frames = 6 }
working = { row = 7, frames = 6 }
error = { row = 5, frames = 8 }
"#;
    #[test]
    fn selects_states_and_falls_back() {
        let pack = Sprite::parse(PACK).unwrap();
        assert_eq!(pack.animation(&serde_json::json!({"pet_state":"working"})).row, 7);
        assert_eq!(pack.animation(&serde_json::json!({"pet_state":"error"})).frames, 8);
        assert_eq!(pack.animation(&serde_json::json!({"pet_state":"unknown"})).row, 0);
        assert_eq!(pack.animation(&serde_json::Value::Null).row, 0);
    }
    #[test]
    fn rejects_unsafe_or_unbounded_packs() {
        for (from, to) in [("cat.png", "../cat.png"), ("cat.png", "{file}"),
            ("frames = 8", "frames = 9"), ("row = 7", "row = 9"),
            ("frame_width = 192", "frame_width = 0"), ("interval_ms = 140", "interval_ms = 1"),
            ("display_height = 28", "display_height = 100"), ("idle =", "rest =")] {
            assert!(Sprite::parse(&PACK.replace(from, to)).is_err(), "{to}");
        }
    }
}
