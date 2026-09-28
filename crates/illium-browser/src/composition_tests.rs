//! Real HWND/Slint/WebView composition checks with a disposable demo profile.
use super::*;

#[test]
#[ignore = "requires Windows desktop and WebView2; run alone"]
fn per_pixel_home_and_owned_palettes_keep_text_and_pages_opaque() {
    unsafe {
        let demo = illium_browser::demo::DemoData::new().unwrap();
        let mut resources = Resources::new().unwrap();
        let mut checked = false;
        run_inner(
            &[],
            false,
            None,
            |_| {
                let app = snapshot().unwrap();
                let assert_host = || {
                    let style = GetWindowLongPtrW(app.hwnd, GWL_EXSTYLE);
                    assert_eq!(style & WS_EX_LAYERED.0 as isize, 0);
                    assert_ne!(style & WS_EX_NOREDIRECTIONBITMAP.0 as isize, 0);
                };
                assert_host();
                let (panel, edit) = {
                    let p = app.picker.borrow();
                    (p.panel, p.edit)
                };
                // The invisible native line and its Slint mirror share the
                // same physical bounds, including IME candidate positioning.
                let mut edit_rect = RECT::default();
                let mut panel_rect = RECT::default();
                GetWindowRect(edit, &mut edit_rect).unwrap();
                GetWindowRect(panel, &mut panel_rect).unwrap();
                let dc = GetDC(Some(edit));
                let font = HFONT(SendMessageW(edit, WM_GETFONT, None, None).0 as *mut _);
                let previous = SelectObject(dc, font.into());
                let mut metrics = TEXTMETRICW::default();
                assert!(GetTextMetricsW(dc, &mut metrics).as_bool());
                SelectObject(dc, previous);
                ReleaseDC(Some(edit), dc);
                assert_eq!(edit_rect.bottom - edit_rect.top, metrics.tmHeight);
                let scale = GetDpiForWindow(panel) as i32;
                let line_center = edit_rect.top + edit_rect.bottom - 2 * panel_rect.top;
                assert!(
                    (line_center - 60 * scale / 96).abs() <= 2,
                    "input must be vertically centered"
                );
                assert!(edit_rect.left - panel_rect.left >= 47 * scale / 96);
                assert_eq!(GetWindow(panel, GW_OWNER).unwrap(), app.hwnd);
                assert_ne!(
                    GetWindowLongPtrW(panel, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize,
                    0
                );
                assert_eq!(GetWindowLongPtrW(panel, GWL_STYLE) & WS_CHILD.0 as isize, 0);
                let mut web_bounds = RECT::default();
                app.controller
                    .as_ref()
                    .unwrap()
                    .Bounds(&mut web_bounds)
                    .unwrap();
                for source in [
                    include_str!("../../../config/themes/catppuccin-mocha.toml"),
                    include_str!("../../../config/themes/catppuccin-latte.toml"),
                ] {
                    let mut theme = illium_theme::Theme::parse(source).unwrap();
                    theme.background_opacity = 0.5;
                    for blur in [false, true] {
                        theme.background_blur = blur;
                        home_mode(true);
                        apply_theme(&theme).unwrap();
                        app.home_surface.borrow_mut().assert_alpha(128, false);
                        app.picker.borrow().assert_alpha(128);
                        assert_host();
                        // Same display paths used by actual navigation and Ctrl+L.
                        home_mode(false);
                        app.home_surface.borrow_mut().assert_alpha(255, false);
                        palette(true);
                        app.picker.borrow().assert_alpha(128);
                        let mut after = RECT::default();
                        app.controller.as_ref().unwrap().Bounds(&mut after).unwrap();
                        assert_eq!(after, web_bounds, "palette must not resize WebView");
                        theme.background_opacity = 0.7;
                        apply_theme(&theme).unwrap();
                        app.home_surface.borrow_mut().assert_alpha(255, false);
                        app.picker.borrow().assert_alpha(179);
                        let _ = SetWindowTextW(edit, w!("Mirror Été 日本語"));
                        SendMessageW(edit, 0x00b1, Some(WPARAM(0)), Some(LPARAM(0)));
                        let plain = app.picker.borrow().input_pixels();
                        let opaque = plain.as_slice().iter().filter(|p| p.a == 255).count();
                        assert!(
                            opaque > 0,
                            "native glyphs must survive alpha extraction in both themes"
                        );
                        assert!(
                            plain.as_slice().iter().any(|p| p.a == 0),
                            "input background must be transparent"
                        );
                        assert!(
                            plain
                                .as_slice()
                                .iter()
                                .all(|p| p.r <= p.a && p.g <= p.a && p.b <= p.a)
                        );
                        SendMessageW(edit, 0x00b1, Some(WPARAM(0)), Some(LPARAM(-1)));
                        let selected = app.picker.borrow().input_pixels();
                        assert!(
                            selected.as_slice().iter().filter(|p| p.a == 255).count() > opaque,
                            "native selection must appear in the mirror"
                        );
                        let _ = SetWindowTextW(edit, w!(""));
                        let mut edit_alpha = 0;
                        GetLayeredWindowAttributes(edit, None, Some(&mut edit_alpha), None)
                            .unwrap();
                        assert_eq!(
                            edit_alpha, 0,
                            "native input must remain visually hidden in every theme"
                        );
                        app.leader.borrow_mut().state.input(Key::Leader, false);
                        sync_leader(&app);
                        {
                            let leader = app.leader_panel.borrow();
                            assert_eq!(GetWindow(leader.hwnd, GW_OWNER).unwrap(), app.hwnd);
                            let menu = app.leader.borrow().state.menu();
                            leader.paint(menu);
                            leader.assert_alpha(179);
                        }
                        app.leader.borrow_mut().state.cancel();
                        sync_leader(&app);
                        theme.background_opacity = 0.5;
                        palette(false);
                        assert!(!IsWindowVisible(panel).as_bool());
                    }
                }
                home_mode(true);
                // Native Unicode editing and native selection coexist with Slint rows.
                let _ = SetWindowTextW(edit, w!("Été 日本語"));
                assert_eq!(app.picker.borrow().text(), "Été 日本語");
                SendMessageW(edit, 0x00b1, Some(WPARAM(0)), Some(LPARAM(-1)));
                SendMessageW(
                    edit,
                    0x00c2,
                    Some(WPARAM(1)),
                    Some(LPARAM(w!("Replacement").0 as isize)),
                );
                assert_eq!(app.picker.borrow().text(), "Replacement");
                SendMessageW(edit, WM_UNDO, None, None);
                assert_eq!(app.picker.borrow().text(), "Été 日本語");
                // A click delivered to the alpha-zero child's parent must land
                // in the real EDIT, not in an independent Slint text editor.
                let click = LPARAM(((30 * scale / 96) << 16 | (49 * scale / 96)) as isize);
                app.picker
                    .borrow()
                    .pointer(WM_LBUTTONDOWN, WPARAM(1), click);
                app.picker.borrow().pointer(WM_LBUTTONUP, WPARAM(0), click);
                assert_eq!(GetFocus(), edit);
                let selection = SendMessageW(edit, 0x00b0, None, None).0 as u32;
                assert_eq!(
                    selection & 0xffff,
                    selection >> 16,
                    "click collapses native selection"
                );
                app.picker
                    .borrow()
                    .pointer(WM_LBUTTONDBLCLK, WPARAM(1), click);
                app.picker.borrow().pointer(WM_LBUTTONUP, WPARAM(0), click);
                let selection = SendMessageW(edit, 0x00b0, None, None).0 as u32;
                assert!(
                    selection & 0xffff < selection >> 16,
                    "double click selects a native word"
                );
                let long = wide(&"https://example.org/été/日本語/".repeat(30));
                let _ = SetWindowTextW(edit, PCWSTR(long.as_ptr()));
                SendMessageW(
                    edit,
                    0x00b1,
                    Some(WPARAM(long.len() - 1)),
                    Some(LPARAM((long.len() - 1) as isize)),
                );
                SendMessageW(edit, 0x00b7, None, None); // EM_SCROLLCARET
                let scrolled = app.picker.borrow().input_pixels();
                // Some Windows EDIT versions return zero for
                // EM_GETFIRSTVISIBLELINE even when a single line is scrolled.
                let first_x = SendMessageW(edit, 0x00d6, Some(WPARAM(0)), None).0 as i16;
                assert!(first_x < 0, "native input scrolls horizontally");
                assert!(scrolled.as_slice().iter().any(|p| p.a == 255));
                SendMessageW(
                    edit,
                    WM_TIMER,
                    Some(WPARAM(crate::input_mirror::BLINK_TIMER)),
                    None,
                );
                let blinked = app.picker.borrow().input_pixels();
                assert_ne!(
                    scrolled.as_slice(),
                    blinked.as_slice(),
                    "mirrored caret must blink at its native scrolled position"
                );
                let _ = SetWindowTextW(edit, w!(""));
                app.picker.borrow_mut().refresh(app.hwnd);
                app.picker.borrow().choose(1);
                assert!(!app.picker.borrow().input().is_empty());
                // Slint hit testing must update the same native selection model
                // and queue exactly one submit, without navigating in this test.
                app.picker.borrow().paint();
                let scale = GetDpiForWindow(panel) as i32;
                let point = LPARAM(((200 * scale / 96) << 16 | (40 * scale / 96)) as isize);
                app.picker
                    .borrow()
                    .pointer(WM_LBUTTONDOWN, WPARAM(1), point);
                app.picker.borrow().pointer(WM_LBUTTONUP, WPARAM(0), point);
                let mut submit = MSG::default();
                assert!(
                    PeekMessageW(&mut submit, Some(app.hwnd), SUBMIT, SUBMIT, PM_REMOVE).as_bool()
                );
                assert!(
                    !PeekMessageW(&mut submit, Some(app.hwnd), SUBMIT, SUBMIT, PM_REMOVE).as_bool()
                );
                assert_eq!(
                    SendMessageW(app.picker.borrow().list, LB_GETCURSEL, None, None).0,
                    1
                );
                app.picker.borrow_mut().show_tabs(app.hwnd);
                assert!(app.picker.borrow().selected_tab().is_some());
                app.picker.borrow().assert_alpha(179);
                let _ = ShowWindow(app.hwnd, SW_MINIMIZE);
                assert!(
                    !IsWindowVisible(panel).as_bool(),
                    "owned palette must hide on minimize"
                );
                let _ = ShowWindow(app.hwnd, SW_RESTORE);
                layout(&app);
                let mut owner = RECT::default();
                let mut popup = RECT::default();
                GetWindowRect(app.hwnd, &mut owner).unwrap();
                GetWindowRect(panel, &mut popup).unwrap();
                assert!(((owner.left + owner.right) - (popup.left + popup.right)).abs() <= 2);
                assert!(((owner.top + owner.bottom) - (popup.top + popup.bottom)).abs() <= 2);
                checked = true;
                let _ = PostMessageW(Some(app.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            Some(demo.path()),
            &mut resources,
        )
        .unwrap();
        assert!(checked);
    }
}
