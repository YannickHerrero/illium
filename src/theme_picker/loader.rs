//! Single preview worker with a replaceable request and bounded memory caches.
//! Independent of the wallpaper worker: browsing cannot cancel a real wallpaper.
use super::render::{self, Frame, Key};
use illium_theme::preview::{self, Entry};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

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
    /// Cumulative snapshot: a slow UI may skip intermediate completions safely.
    Progress(Vec<Option<Arc<Frame>>>),
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
enum PrepareError {
    Unreadable(Entry, String),
    Failed(String),
}
fn prepare_card(
    key: &Key,
    thumbnail: Option<Arc<image::RgbaImage>>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(Arc<image::RgbaImage>, Arc<Frame>), PrepareError> {
    if cancelled() {
        return Err(PrepareError::Failed("preview superseded".into()));
    }
    let thumbnail = match thumbnail {
        Some(image) => image,
        None => Arc::new(
            super::disk_cache::thumbnail(&key.entry)
                .map_err(|e| PrepareError::Unreadable(key.entry.clone(), e))?,
        ),
    };
    if cancelled() {
        return Err(PrepareError::Failed("preview superseded".into()));
    }
    let frame = Arc::new(render::card(&thumbnail, key).map_err(PrepareError::Failed)?);
    Ok((thumbnail, frame))
}
impl Default for Loader {
    fn default() -> Self {
        let mut thumbnails: Cache<Entry, image::RgbaImage> = Cache::default();
        let mut frames: Cache<Key, Frame> = Cache::default();
        Self::start(move |job, cancelled, publish| match job {
            Job::Scan(home) => preview::catalog(&home).map(Output::Catalog),
            Job::Wallpapers(home, theme) => preview::wallpapers(&home, &theme).map(Output::Catalog),
            Job::Render(keys) => {
                if keys.len() > 33 {
                    return Err("preview request exceeds 33 visible cards".into());
                }
                let mut out = vec![None; keys.len()];
                let mut bytes = 0;
                // Center alone first; then at most two simultaneous decodes.
                // Cache hits never spawn threads. Caches stay worker-owned.
                let mut work = keys.into_iter().enumerate().rev().peekable();
                let mut first = true;
                while work.peek().is_some() {
                    if cancelled() {
                        return Err("preview superseded".into());
                    }
                    let count = if first { 1 } else { 2 };
                    first = false;
                    let mut missing = Vec::new();
                    for (index, key) in work.by_ref().take(count) {
                        if let Some(frame) = frames.get(&key) {
                            bytes += frame.bytes();
                            out[index] = Some(frame);
                        } else {
                            let thumbnail = thumbnails.get(&key.entry);
                            missing.push((index, key, thumbnail));
                        }
                    }
                    let results = std::thread::scope(|scope| {
                        let mut threads = Vec::new();
                        // With a single miss, use the existing worker directly.
                        if missing.len() == 1 {
                            let (i, key, thumbnail) = missing.pop().unwrap();
                            let result = prepare_card(&key, thumbnail, cancelled);
                            return vec![(i, key, result)];
                        }
                        for (i, key, thumbnail) in missing {
                            threads.push(scope.spawn(move || {
                                let result = prepare_card(&key, thumbnail, cancelled);
                                (i, key, result)
                            }));
                        }
                        threads
                            .into_iter()
                            .map(|t| t.join().expect("preview worker panicked"))
                            .collect()
                    });
                    for (index, key, result) in results {
                        let (thumbnail, frame) = match result {
                            Ok(prepared) => prepared,
                            Err(PrepareError::Unreadable(entry, error)) => {
                                return Ok(Output::Unreadable(entry, error));
                            }
                            Err(PrepareError::Failed(error)) => return Err(error),
                        };
                        // A RAM thumbnail hit already occupies its cache slot.
                        if thumbnails.get(&key.entry).is_none() {
                            thumbnails.insert(
                                key.entry.clone(),
                                thumbnail.clone(),
                                thumbnail.as_raw().len(),
                            );
                        }
                        frames.insert(key, frame.clone(), frame.bytes());
                        bytes += frame.bytes();
                        out[index] = Some(frame);
                    }
                    if bytes > MAX_VIEW_BYTES {
                        return Err("preview view exceeds 256 MiB".into());
                    }
                    publish(Output::Progress(out.clone()));
                }
                Ok(Output::Frames(out.into_iter().flatten().collect()))
            }
        })
    }
}
impl Loader {
    fn start(
        mut prepare: impl FnMut(
            Job,
            &(dyn Fn() -> bool + Sync),
            &dyn Fn(Output),
        ) -> Result<Output, String>
        + Send
        + 'static,
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
                let publish = |output| {
                    let mut state = worker.state.lock().unwrap();
                    if !state.closed && state.serial == serial {
                        state.result = Some(Completion {
                            serial,
                            result: Ok(output),
                        });
                    }
                };
                let result = prepare(job, &cancelled, &publish);
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
        let loader = Loader::start(move |_, cancelled, _| {
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
    fn progress_is_available_before_completion_and_cancel_discards_it() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let loader = Loader::start(move |_, _, publish| {
            publish(Output::Progress(vec![None]));
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(Output::Frames(vec![]))
        });
        let serial = loader.request(Job::Render(vec![]));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let result = loader.take_result().unwrap();
        assert_eq!(result.serial, serial);
        assert!(matches!(result.result, Ok(Output::Progress(_))));
        loader.cancel();
        release_tx.send(()).unwrap();
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
    fn parallel_render_preserves_paint_order_and_reuses_frames() {
        let dir = std::env::temp_dir().join(format!("picker-parallel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let keys: Vec<_> = (0..3)
            .map(|i| {
                let path = dir.join(format!("{i}.png"));
                image::RgbaImage::from_pixel(16, 9, image::Rgba([i * 60, 20, 30, 255]))
                    .save(&path)
                    .unwrap();
                Key {
                    entry: Entry {
                        id: i.to_string(),
                        path,
                        size: 1,
                        modified: 1,
                    },
                    dpi: 96,
                    selected: i == 2,
                    colors: render::Colors::from_theme(&illium_theme::Theme::default_theme()),
                }
            })
            .collect();
        let loader = Loader::default();
        let frames = || loop {
            match wait(&loader).result.unwrap() {
                Output::Progress(_) => {}
                Output::Frames(frames) => break frames,
                _ => panic!("unexpected output"),
            }
        };
        loader.request(Job::Render(keys.clone()));
        let first = frames();
        assert_eq!(first.len(), 3);
        for (frame, key) in first.iter().zip(&keys) {
            let expected = render::card(&render::thumbnail(&key.entry).unwrap(), key).unwrap();
            assert_eq!(frame.pixels, expected.pixels);
        }
        loader.request(Job::Render(keys));
        let second = frames();
        assert!(first.iter().zip(second).all(|(a, b)| Arc::ptr_eq(a, &b)));
        std::fs::remove_dir_all(dir).unwrap();
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
