//! Command-line targets and the backward-compatible resident open payload.
use serde::Deserialize;

pub const MAX_TABS: usize = 32;

pub fn targets(mut inputs: Vec<String>) -> Result<Vec<String>, String> {
    if inputs.len() > MAX_TABS {
        return Err(format!("Open at most {MAX_TABS} tabs per launch"));
    }
    if inputs.is_empty() { inputs.push(String::new()); }
    Ok(inputs)
}

pub fn open_command(inputs: &[String]) -> Result<String, String> {
    let inputs = targets(inputs.to_vec())?;
    // Keep the single-target wire format compatible with older residents.
    let payload = if inputs.len() == 1 {
        serde_json::to_string(&inputs[0])
    } else {
        serde_json::to_string(&inputs)
    }.map_err(|e| e.to_string())?;
    let command = format!("open {payload}");
    if command.len() > winarchy_ipc::protocol::MAX_COMMAND_BYTES {
        return Err("Browser launch exceeds the IPC command size limit".into());
    }
    Ok(command)
}

pub fn parse_open(payload: &str) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input { One(String), Many(Vec<String>) }
    let values = match serde_json::from_str(payload).map_err(|e| e.to_string())? {
        Input::One(value) => vec![value],
        Input::Many(values) => values,
    };
    targets(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_legacy_launches_remain_supported() {
        assert_eq!(targets(vec![]).unwrap(), [""]);
        assert_eq!(parse_open("\"\"").unwrap(), [""]);
        assert_eq!(open_command(&[]).unwrap(), "open \"\"");
        assert_eq!(parse_open("[]").unwrap(), [""]);
    }
    #[test]
    fn multiple_targets_preserve_order_unicode_and_quoted_searches() {
        let inputs = vec!["https://example.com/?a=1&b=2".into(), "日本語 with spaces".into(), "about:blank".into()];
        let wire = open_command(&inputs).unwrap();
        assert_eq!(parse_open(wire.strip_prefix("open ").unwrap()).unwrap(), inputs);
    }
    #[test]
    fn invalid_or_excessive_requests_are_rejected_before_opening() {
        for invalid in ["null", "{}", "[1]", "[\"unterminated"] {
            assert!(parse_open(invalid).is_err());
        }
        assert!(open_command(&vec![String::new(); MAX_TABS + 1]).is_err());
        assert!(parse_open(&serde_json::to_string(&vec![""; MAX_TABS + 1]).unwrap()).is_err());
        assert!(open_command(&["x".repeat(8192)]).is_err());
    }
}
