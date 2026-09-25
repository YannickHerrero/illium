/// No console in this subsystem: failures go to a file next to the user's temp data.
pub fn write(message: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("illium-apps.log"))
    {
        let _ = writeln!(f, "{message}");
    }
}
