//! Hidden test-owned windows only; never stops Explorer or installs a hook.
use super::{native, session};
use windows::{Win32::UI::WindowsAndMessaging::*, core::PCWSTR};
#[test]
fn stale_generation_cannot_own_a_retagged_window() {
    unsafe {
        session::test_initialize().unwrap();
        let class = native::wide("STATIC");
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            None,
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let id = h.0 as isize;
        let first = session::tag(id).unwrap();
        assert!(session::owns(id, first));
        let second = session::tag(id).unwrap();
        assert_ne!(first, second);
        assert!(!session::owns(id, first));
        assert!(session::owns(id, second));
        session::untag(id);
        assert!(!session::owns(id, second));
        let _ = DestroyWindow(h);
    }
}
