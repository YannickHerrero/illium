//! Exercise the production lock surface without a desktop or global input hook.
use super::*;
use slint::platform::{
    Platform, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{cell::RefCell, time::Duration};

struct Headless(Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.0.borrow_mut() = Some(window.clone());
        Ok(window)
    }
}

#[test]
fn focused_key_wakes_without_hook_and_cannot_type_or_submit() {
    let adapter = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Headless(adapter.clone()))).unwrap();
    let (tx, rx) = crate::queue::channel(32);
    let mut lock = Lock::new(tx);
    lock.opened = true;
    lock.epoch.set(7);
    let view = lock.view().unwrap();
    view.set_primary(true);
    view.set_ready(true);
    view.set_surface_width(800.);
    view.set_surface_height(600.);
    view.show().unwrap();
    lock.views.push(view.clone_strong());
    let window = adapter.borrow().as_ref().unwrap().clone();
    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    let press = |text: slint::SharedString| window.dispatch_event(WindowEvent::KeyPressed { text });
    let release =
        |text: slint::SharedString| window.dispatch_event(WindowEvent::KeyReleased { text });
    for key in [
        slint::SharedString::from("a"),
        slint::platform::Key::Return.into(),
        slint::platform::Key::Escape.into(),
    ] {
        let now = Instant::now();
        let mut saver = Saver::new(Default::default(), now - Duration::from_secs(31), 0, 1);
        assert!(saver.poll(now, 0));
        lock.saver = Some(saver);
        input::SAVING.store(true, Ordering::SeqCst);
        view.set_saving(true);
        view.invoke_focus_saver();
        press(key.clone());
        let Event::Lock(epoch, Input::Wake) = rx.try_recv().expect("focused key must request wake")
        else {
            panic!("not a wake event")
        };
        // A stale event must not dismiss a later lock session.
        assert_eq!(lock.input(epoch - 1, Input::Wake), Outcome::None);
        assert!(view.get_saving());
        assert_eq!(lock.input(epoch, Input::Wake), Outcome::None);
        assert!(lock.opened);
        assert!(!view.get_saving());
        assert!(!input::SAVING.load(Ordering::SeqCst));
        assert!(lock.saver.as_ref().unwrap().effect.is_none());
        // Repeats remain in the fallback scope until the waking key is up.
        press(key.clone());
        press(key.clone());
        assert!(rx.try_recv().is_err());
        release(key);
        press("b".into());
        release("b".into());
        press(slint::platform::Key::Return.into());
        release(slint::platform::Key::Return.into());
        let Event::Lock(_, Input::Submit(password)) =
            rx.try_recv().expect("password field regains focus")
        else {
            panic!("not a password submission")
        };
        assert_eq!(
            password, "b",
            "waking key/repeats must never enter the password"
        );
        assert!(rx.try_recv().is_err());
    }
    lock.stop_saver();
    view.hide().unwrap();
}
