//! One coalesced UI callback regardless of how many hook/worker events arrive.
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

thread_local! {
    static DRAIN: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}
static POSTED: AtomicBool = AtomicBool::new(false);

pub fn install(drain: impl Fn() + 'static) {
    DRAIN.with(|slot| *slot.borrow_mut() = Some(Rc::new(drain)));
}

pub fn wake() {
    if POSTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let requested = std::time::Instant::now();
    if slint::invoke_from_event_loop(move || {
        tracing::trace!(
            wake_latency_us = requested.elapsed().as_micros(),
            "event-loop wake delivered"
        );
        // Clear before draining: a concurrent producer can request the next
        // turn. Cloning releases the TLS borrow before invoking user code.
        POSTED.store(false, Ordering::Release);
        let drain = DRAIN.with(|slot| slot.borrow().clone());
        if let Some(drain) = drain {
            drain();
        }
    })
    .is_err()
    {
        POSTED.store(false, Ordering::Release);
    }
}

pub fn clear() {
    DRAIN.with(|slot| *slot.borrow_mut() = None);
}
