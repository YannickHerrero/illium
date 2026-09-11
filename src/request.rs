//! A queued IPC command can be cancelled, but an already-started side effect cannot.
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::Sender,
    },
    time::Instant,
};
const QUEUED: u8 = 0;
const RUNNING: u8 = 1;
const CANCELLED: u8 = 2;
pub struct Ticket {
    deadline: Instant,
    state: AtomicU8,
}
impl Ticket {
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            state: AtomicU8::new(QUEUED),
        }
    }
    pub fn start(&self, now: Instant) -> bool {
        if now >= self.deadline {
            self.cancel();
            return false;
        }
        self.state
            .compare_exchange(QUEUED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn cancel(&self) -> bool {
        self.state
            .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}
pub struct ReplyTo {
    pub ticket: Arc<Ticket>,
    pub sender: Sender<Result<String, String>>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn cancelled_never_runs() {
        let now = Instant::now();
        let t = Ticket::new(now + Duration::from_secs(1));
        assert!(t.cancel());
        assert!(!t.start(now));
    }
    #[test]
    fn expired_never_runs() {
        let now = Instant::now();
        let t = Ticket::new(now);
        assert!(!t.start(now));
    }
    #[test]
    fn starts_at_most_once() {
        let now = Instant::now();
        let t = Ticket::new(now + Duration::from_secs(1));
        assert!(t.start(now));
        assert!(!t.start(now));
        assert!(!t.cancel());
    }
    #[test]
    fn cancellation_and_start_are_exclusive() {
        for _ in 0..100 {
            let now = Instant::now();
            let t = Arc::new(Ticket::new(now + Duration::from_secs(30)));
            let other = t.clone();
            let worker = std::thread::spawn(move || other.cancel());
            let started = t.start(now);
            assert_ne!(started, worker.join().unwrap());
        }
    }
}
