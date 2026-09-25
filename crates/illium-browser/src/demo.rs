//! Disposable browser data. The real config home is used only for live theming.
use std::{io, path::Path};

pub struct DemoData(tempfile::TempDir);
impl DemoData {
    pub fn new() -> io::Result<Self> {
        let dir = tempfile::Builder::new().prefix("illium-demo-").tempdir()?;
        let data = dir.path().join("browser");
        std::fs::create_dir(&data)?;
        std::fs::create_dir(dir.path().join("profile"))?;
        // Blocker requires a local list. Never load/download the personal lists.
        std::fs::write(
            data.join("custom.txt"),
            "! Demo fixture only\n||ads.example^\n",
        )?;
        std::fs::write(
            data.join("library.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "bookmarks": [
                    {"url": "https://www.rust-lang.org/", "title": "Rust Programming Language"},
                    {"url": "https://docs.rs/", "title": "Rust crate documentation"},
                    {"url": "https://github.com/", "title": "GitHub"}
                ],
                "history": [
                    {"url": "https://doc.rust-lang.org/book/", "title": "The Rust Programming Language"},
                    {"url": "https://developer.mozilla.org/", "title": "MDN Web Docs"}
                ]
            }))?,
        )?;
        Ok(Self(dir))
    }
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}
impl Drop for DemoData {
    fn drop(&mut self) {
        // WebView's child processes can briefly retain profile files after Close.
        for _ in 0..20 {
            match std::fs::remove_dir_all(self.path()) {
                Ok(()) => return,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        // TempDir makes one final best-effort attempt. No normal profile is touched.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;
    #[test]
    fn fixtures_are_disposable_and_independent() {
        let first = DemoData::new().unwrap();
        let second = DemoData::new().unwrap();
        assert_ne!(first.path(), second.path());
        assert!(crate::Blocker::load(&first.path().join("browser")).is_ok());
        let mut a = Library::load(&first.path().join("browser")).unwrap();
        assert_eq!(a.suggestions("").len(), 5);
        a.add_bookmark("https://example.com/private", "Not shared")
            .unwrap();
        let b = Library::load(&second.path().join("browser")).unwrap();
        assert!(b.suggestions("Not shared").is_empty());
        let path = first.path().to_owned();
        drop(first);
        assert!(!path.exists());
        assert!(second.path().join("profile").is_dir());
    }
}
