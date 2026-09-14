//! Bar clock formatting with a small strftime-like token set, in English.
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SHORT_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "June", "July", "Aug", "Sept", "Oct", "Nov", "Dec",
];
pub const TOKENS: [&str; 8] = ["%A", "%a", "%d", "%B", "%b", "%H", "%M", "%S"];
/// Local date and time fields as Windows reports them: `weekday` 0 is Sunday,
/// `month` 1 is January.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moment {
    pub weekday: u8,
    pub day: u8,
    pub month: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
pub fn format(pattern: &str, t: Moment) -> String {
    let day = DAYS[usize::from(t.weekday) % 7];
    let month = usize::from(t.month.clamp(1, 12)) - 1;
    pattern
        .replace("%A", day)
        .replace("%a", &day[..3])
        .replace("%d", &format!("{:02}", t.day))
        .replace("%B", MONTHS[month])
        .replace("%b", SHORT_MONTHS[month])
        .replace("%H", &format!("{:02}", t.hour))
        .replace("%M", &format!("{:02}", t.minute))
        .replace("%S", &format!("{:02}", t.second))
}
/// Rejects unknown `%` tokens so a typo does not show up verbatim in the bar.
pub fn validate(pattern: &str) -> Result<(), String> {
    let mut rest = pattern.to_owned();
    for token in TOKENS {
        rest = rest.replace(token, "");
    }
    if rest.contains('%') {
        return Err(format!("clock_format supports only {}", TOKENS.join(", ")));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_pattern() {
        let t = Moment {
            weekday: 1,
            day: 14,
            month: 9,
            hour: 9,
            minute: 51,
            second: 7,
        };
        assert_eq!(format("%A %d %b - %H:%M", t), "Monday 14 Sept - 09:51");
        assert_eq!(format("%a %B %S", t), "Mon September 07");
        assert_eq!(format("%H:%M", t), "09:51");
    }
    #[test]
    fn unknown_tokens_are_rejected() {
        assert!(validate("%A %d %b - %H:%M").is_ok());
        assert!(validate("%Y-%m").is_err());
        assert!(validate("100%").is_err());
    }
}
