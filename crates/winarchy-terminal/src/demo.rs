//! Fixed screenshot fixtures: no shell, environment, filesystem or machine probes.
use crate::model::{Model, Size};

#[derive(Clone, Copy, Debug)]
pub enum Scene {
    Build,
    Fetch,
}
impl Scene {
    pub fn model(self, size: Size) -> Model {
        let mut model = Model::new(size, 100);
        model.configure(100, false);
        let mut output = String::from("\x1b[?25l"); // Stable screenshot, no cursor.
        match self {
            Self::Build => output.push_str(concat!(
                "\x1b[36m demo@winarchy\x1b[0m \x1b[34m~/projects/hello-rust\x1b[0m\r\n",
                " $ cargo build --release\r\n\r\n",
                "\x1b[32m   Compiling\x1b[0m proc-macro2 v1.0.95\r\n",
                "\x1b[32m   Compiling\x1b[0m unicode-ident v1.0.18\r\n",
                "\x1b[32m   Compiling\x1b[0m quote v1.0.40\r\n",
                "\x1b[32m   Compiling\x1b[0m syn v2.0.101\r\n",
                "\x1b[32m   Compiling\x1b[0m serde v1.0.219\r\n",
                "\x1b[32m   Compiling\x1b[0m serde_json v1.0.140\r\n",
                "\x1b[32m   Compiling\x1b[0m hello-rust v0.1.0\r\n",
                "\x1b[32m    Finished\x1b[0m release [optimized] in 4.28s\r\n\r\n",
                " $ cargo test\r\n\r\n",
                "\x1b[32m     Running\x1b[0m unittests src/main.rs\r\n",
                " running 4 tests\r\n",
                " test palette::colors ... \x1b[32mok\x1b[0m\r\n",
                " test layout::split ... \x1b[32mok\x1b[0m\r\n",
                " test config::parse ... \x1b[32mok\x1b[0m\r\n",
                " test theme::reload ... \x1b[32mok\x1b[0m\r\n\r\n",
                " test result: \x1b[32mok\x1b[0m. 4 passed; 0 failed\r\n\r\n",
                "\x1b[33m Demo output\x1b[0m - no project was compiled.\r\n",
                "\x1b[90m Ready for your next idea.\x1b[0m\r\n",
            )),
            Self::Fetch => {
                output.push_str(concat!(
                    "\x1b[36m  /\\    /\\     \x1b[1mdemo@winarchy\x1b[0m\r\n",
                    "\x1b[36m /  \\  /  \\    \x1b[0m-------------\r\n",
                    "\x1b[36m \\   \\/   /    \x1b[34mOS\x1b[0m       Windows 11\r\n",
                    "\x1b[36m  \\      /     \x1b[34mDesktop\x1b[0m  Winarchy\r\n",
                    "\x1b[36m   \\ /\\ /      \x1b[34mTerminal\x1b[0m Winarchy Terminal\r\n",
                    "\x1b[36m    V  V       \x1b[34mHost\x1b[0m     Demo workstation\r\n",
                    "               \x1b[34mCPU\x1b[0m      8 virtual cores\r\n",
                    "               \x1b[34mMemory\x1b[0m   4 GiB / 16 GiB\r\n\r\n",
                ));
                for base in [40, 100] {
                    output.push_str("               ");
                    for color in base..base + 8 {
                        output.push_str(&format!("\x1b[{color}m  "));
                    }
                    output.push_str("\x1b[0m\r\n");
                }
                output.push_str("\r\n\x1b[90m  Demo fetch - fictional system information\x1b[0m");
            }
        }
        model.feed(output.as_bytes());
        model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        index::{Column, Line, Point},
        term::TermMode,
    };
    #[test]
    fn fixtures_are_stable_and_have_no_cursor_or_clipboard_output() {
        for scene in [Scene::Build, Scene::Fetch] {
            let mut model = scene.model(Size::new(90, 40));
            let text = model.term.bounds_to_string(
                Point::new(Line(0), Column(0)),
                Point::new(Line(39), Column(89)),
            );
            assert!(text.contains("demo@winarchy"));
            assert!(text.contains("Demo"));
            assert!(!model.term.mode().contains(TermMode::SHOW_CURSOR));
            assert!(model.take_clipboard_write().is_none());
            assert!(model.error.is_none());
        }
    }
}
