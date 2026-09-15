//! Single preview worker with a replaceable request and bounded memory caches.
//! Independent of the wallpaper worker: browsing cannot cancel a real wallpaper.
use super::render::{self, Frame, Key};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};
use winarchy_theme::preview::{self, Entry};

const CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_VIEW_BYTES: usize = 256 * 1024 * 1024;

struct Cache<K, V> {
    rows: VecDeque<(K, Arc<V>, usize)>,
    bytes: usize,
}
impl<K: PartialEq, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self {
            rows: VecDeque::new(),
            bytes: 0,
        }
    }
}
impl<K: PartialEq, V> Cache<K, V> {
    fn get(&mut self, key: &K) -> Option<Arc<V>> {
        let i = self.rows.iter().position(|(k, _, _)| k == key)?;
        let row = self.rows.remove(i)?;
        let value = row.1.clone();
        self.rows.push_back(row);
        Some(value)
    }
    fn insert(&mut self, key: K, value: Arc<V>, bytes: usize) {
        if bytes > CACHE_BYTES {
            return;
        }
        while self.bytes + bytes > CACHE_BYTES {
            let Some((_, _, size)) = self.rows.pop_front() else {
                break;
            };
            self.bytes -= size;
        }
        self.rows.push_back((key, value, bytes));
        self.bytes += bytes;
    }
}
#[derive(Clone)]
pub enum Job {
    Scan(PathBuf),
    Wallpapers(PathBuf, String),
    Render(Vec<Key>),
}
pub enum Output {
    Catalog(Vec<Entry>),
    Frames(Vec<Arc<Frame>>),
    Unreadable(Entry, String),
}
pub struct Completion {
    pub serial: u64,
    pub result: Result<Output, String>,
}
struct State {
    serial: u64,
    pending: Option<Job>,
    result: Option<Completion>,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
pub struct Loader {
    shared: Arc<Shared>,
}
impl Default for Loader {
    fn default() -> Self {
        let mut thumbnails: Cache<Entry, image::RgbaImage> = Cache::default();
        let mut frames: Cache<Key, Frame> = Cache::default();
        Self::start(move |job, cancelled| match job {
            Job::Scan(home) => preview::catalog(&home).map(Output::Catalog),
            Job::Wallpapers(home, theme) => preview::wallpapers(&home, &theme).map(Output::Catalog),
            Job::Render(keys) => {
                if keys.len() > 33 {
                    return Err("preview request exceeds 33 visible cards".into());
                }
                let mut out = vec![];
                let mut bytes = 0;
                for key in keys {
                    if cancelled() {
                        return Err("preview superseded".into());
                    }
                    let frame = if let Some(frame) = frames.get(&key) {
                        frame
                    } else {
                        let thumbnail = if let Some(image) = thumbnails.get(&key.entry) {
                            image
                        } else {
                            let image = match render::thumbnail(&key.entry) {
                                Ok(image) => Arc::new(image),
                                Err(error) => return Ok(Output::Unreadable(key.entry, error)),
                            };
                            thumbnails.insert(
                                key.entry.clone(),
                                image.clone(),
                                image.as_raw().len(),
                            );
                            image
                        };
                        if cancelled() {
                            return Err("preview superseded".into());
                        }
                        let frame = Arc::new(render::card(&thumbnail, &key)?);
                        frames.insert(key.clone(), frame.clone(), frame.bytes());
                        frame
                    };
                    bytes += frame.bytes();
                    if bytes > MAX_VIEW_BYTES {
                        return Err("preview view exceeds 256 MiB".into());
                    }
                    out.push(frame);
                }
                Ok(Output::Frames(out))
            }
        })
    }
}
impl Loader {
    fn start(
        mut prepare: impl FnMut(Job, &dyn Fn() -> bool) -> Result<Output, String> + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                serial: 0,
                pending: None,
                result: None,
                closed: false,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        std::thread::spawn(move || {
            loop {
                let mut state = worker.state.lock().unwrap();
                while state.pending.is_none() && !state.closed {
                    state = worker.wake.wait(state).unwrap();
                }
                if state.closed {
                    break;
                }
                let job = state.pending.take().unwrap();
                let serial = state.serial;
                drop(state);
                let cancelled = || {
                    let s = worker.state.lock().unwrap();
                    s.closed || s.serial != serial
                };
                let result = prepare(job, &cancelled);
                let mut state = worker.state.lock().unwrap();
                if !state.closed && state.serial == serial {
                    state.result = Some(Completion { serial, result });
                }
            }
        });
        Self { shared }
    }
    pub fn request(&self, job: Job) -> u64 {
        let mut s = self.shared.state.lock().unwrap();
        s.serial = s.serial.wrapping_add(1);
        s.result = None;
        s.pending = Some(job);
        let serial = s.serial;
        self.shared.wake.notify_one();
        serial
    }
    pub fn cancel(&self) {
        let mut s = self.shared.state.lock().unwrap();
        s.serial = s.serial.wrapping_add(1);
        s.pending = None;
        s.result = None;
    }
    pub fn take_result(&self) -> Option<Completion> {
        self.shared.state.lock().unwrap().result.take()
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        let mut s = self.shared.state.lock().unwrap();
        s.closed = true;
        s.pending = None;
        self.shared.wake.notify_one();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    fn wait(loader: &Loader) -> Completion {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = loader.take_result() {
                return result;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn superseded_and_cancelled_work_cannot_publish() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut first = true;
        let loader = Loader::start(move |_, cancelled| {
            if first {
                first = false;
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                assert!(cancelled());
            }
            Ok(Output::Catalog(vec![]))
        });
        loader.request(Job::Scan("old".into()));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        loader.cancel();
        let serial = loader.request(Job::Scan("new".into()));
        release_tx.send(()).unwrap();
        assert_eq!(wait(&loader).serial, serial);
        loader.cancel();
        assert!(loader.take_result().is_none());
    }
    #[test]
    fn cache_is_lru_and_byte_bounded() {
        let mut cache = Cache::default();
        cache.insert(1, Arc::new(1), CACHE_BYTES / 2);
        cache.insert(2, Arc::new(2), CACHE_BYTES / 2);
        assert_eq!(*cache.get(&1).unwrap(), 1);
        cache.insert(3, Arc::new(3), CACHE_BYTES / 2);
        assert!(cache.get(&2).is_none());
        assert!(cache.get(&1).is_some());
        cache.insert(4, Arc::new(4), CACHE_BYTES + 1);
        assert!(cache.get(&4).is_none());
        assert!(cache.bytes <= CACHE_BYTES);
    }
    #[test]
    fn empty_catalog_and_corrupt_images_report_without_hanging() {
        let loader = Loader::default();
        loader.request(Job::Scan("/not/a/configuration".into()));
        assert!(wait(&loader).result.is_err());
        loader.request(Job::Render(vec![]));
        assert!(matches!(wait(&loader).result, Ok(Output::Frames(f)) if f.is_empty()));
    }
}
