use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "Raw")]
pub struct Notifications {
    /// Seconds a toast stays on screen unless clicked away.
    pub timeout: u64,
}
impl Default for Notifications {
    fn default() -> Self {
        Self { timeout: 6 }
    }
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Raw {
    timeout: u64,
}
impl Default for Raw {
    fn default() -> Self {
        Self {
            timeout: Notifications::default().timeout,
        }
    }
}
impl TryFrom<Raw> for Notifications {
    type Error = String;
    fn try_from(raw: Raw) -> Result<Self, String> {
        if !(1..=3600).contains(&raw.timeout) {
            return Err("notifications timeout must be 1..=3600 seconds".into());
        }
        Ok(Self {
            timeout: raw.timeout,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_validation() {
        let global: crate::config::Global = toml::from_str("theme = 'nord'").unwrap();
        assert_eq!(global.notifications.timeout, 6);
        let c: Notifications = toml::from_str("timeout = 12").unwrap();
        assert_eq!(c.timeout, 12);
        for s in ["timeout = 0", "timeout = 3601", "sound = true"] {
            assert!(toml::from_str::<Notifications>(s).is_err(), "{s}");
        }
    }
}
