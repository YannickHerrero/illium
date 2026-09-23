//! Hook callbacks must never block waiting for the WM or allocate unbounded queues.
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
pub struct Sender<T> {
    inner: SyncSender<T>,
    overflow: Arc<AtomicBool>,
    wake: Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>,
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            overflow: self.overflow.clone(),
            wake: self.wake.clone(),
        }
    }
}
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::sync_channel(capacity);
    (
        Sender {
            inner: tx,
            overflow: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(OnceLock::new()),
        },
        rx,
    )
}
impl<T> Sender<T> {
    /// Install after the consumer is ready. The notifier must be nonblocking
    /// and coalesce notifications; it runs on the sending (possibly hook) thread.
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        assert!(
            self.wake.set(Box::new(wake)).is_ok(),
            "waker already installed"
        );
    }
    pub fn send(&self, value: T) -> Result<(), TrySendError<T>> {
        let result = self.inner.try_send(value);
        if matches!(result, Err(TrySendError::Full(_))) {
            self.overflow.store(true, Ordering::Release);
        }
        if !matches!(result, Err(TrySendError::Disconnected(_)))
            && let Some(wake) = self.wake.get()
        {
            wake();
        }
        result
    }
    pub fn take_overflow(&self) -> bool {
        self.overflow.swap(false, Ordering::AcqRel)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_nonblocking() {
        let (tx, rx) = channel(2);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        assert!(matches!(tx.send(3), Err(TrySendError::Full(3))));
        assert!(tx.clone().take_overflow());
        assert!(!tx.take_overflow());
        assert_eq!(rx.recv().unwrap(), 1);
        tx.send(4).unwrap();
        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 4);
    }
    #[test]
    fn wakes_after_publication_and_on_overflow_not_disconnect() {
        use std::sync::atomic::AtomicUsize;
        let (tx, rx) = channel(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        tx.set_waker(move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        tx.clone().send(1).unwrap();
        assert!(tx.send(2).is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(rx.recv().unwrap(), 1);
        drop(rx);
        assert!(tx.send(3).is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn disconnect_does_not_request_rescan() {
        let (tx, rx) = channel(1);
        drop(rx);
        assert!(matches!(tx.send(1), Err(TrySendError::Disconnected(1))));
        assert!(!tx.take_overflow());
    }
}
