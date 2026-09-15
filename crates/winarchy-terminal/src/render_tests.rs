//! Real DirectComposition/DirectWrite tests on a hidden, disposable HWND.
//! No focus changes, clipboard access, WSL start or visible desktop interaction.
use super::*;
use windows::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}
#[test]
fn hidden_gpu_surface_survives_resize_and_font_changes() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: w!("WinarchyTerminalRenderTest"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            class.lpszClassName,
            w!("hidden test"),
            WS_POPUP,
            0,
            0,
            320,
            160,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let config = Config::default();
        let g = Graphics::new(&config).unwrap();
        let mut surface = Surface::new(g.clone(), hwnd, 320, 160, 96, 4).unwrap();
        let mut theme = winarchy_theme::Theme::default_theme();
        theme.terminal_background_opacity = 0.85;
        let p = Palette::new(&theme);
        surface.draw(&Frame::new(None, &p)).unwrap();
        let mut model = Model::new(Size::new(30, 10), 20);
        model.feed("é界 e\u{301} \u{f120}\r\n\x1b[1;3;4;31mstyled\x1b[0m".as_bytes());
        for (w, h, dpi) in [
            (640, 320, 96),
            (800, 600, 144),
            (320, 160, 192),
            (320, 160, 96),
        ] {
            surface.resize(w, h, dpi).unwrap();
            model.term.resize(surface.size());
            surface.draw(&Frame::new(Some(&model), &p)).unwrap();
        }
        let config = Config {
            font_size: 18.,
            ..config
        };
        surface.fonts = g.fonts(&config).unwrap();
        surface.draw(&Frame::new(Some(&model), &p)).unwrap();
        assert_eq!(surface.cell_at(-50, -50), (0, 0));
        drop(surface);
        drop(g);
        DestroyWindow(hwnd).unwrap();
    }
}
