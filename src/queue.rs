//! Hook callbacks must never block waiting for the WM or allocate unbounded queues.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
pub struct Sender<T> {
    inner: SyncSender<T>,
    overflow: Arc<AtomicBool>,
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            overflow: self.overflow.clone(),
        }
    }
}
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::sync_channel(capacity);
    (
        Sender {
            inner: tx,
            overflow: Arc::new(AtomicBool::new(false)),
        },
        rx,
    )
}
impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), TrySendError<T>> {
        let result = self.inner.try_send(value);
        if matches!(result, Err(TrySendError::Full(_))) {
            self.overflow.store(true, Ordering::Release);
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
    fn disconnect_does_not_request_rescan() {
        let (tx, rx) = channel(1);
        drop(rx);
        assert!(matches!(tx.send(1), Err(TrySendError::Disconnected(1))));
        assert!(!tx.take_overflow());
    }
}
