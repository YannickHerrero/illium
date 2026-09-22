//! One bounded worker, a latest-request slot and an LRU of native-sized pixels.
//! No Slint objects cross threads. The UI only uploads already prepared buffers.
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

const CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PREPARED_BYTES: usize = 256 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub path: PathBuf,
    pub size: u64,
    pub modified: u128,
    pub screens: Vec<(u32, u32)>,
}
#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
#[derive(Debug)]
pub struct Prepared {
    pub frames: Vec<Arc<Frame>>,
    bytes: usize,
}
type Pixels = Arc<Prepared>;
type ResultPixels = Result<Pixels, String>;
type Completion = (Key, ResultPixels);
struct Cache {
    entries: VecDeque<(Key, Pixels)>,
    bytes: usize,
    limit: usize,
}
impl Cache {
    fn get(&mut self, key: &Key) -> Option<Pixels> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(index)?;
        let pixels = entry.1.clone();
        self.entries.push_back(entry);
        Some(pixels)
    }
    fn insert(&mut self, key: Key, pixels: Pixels) {
        if pixels.bytes > self.limit {
            return;
        }
        // Different edits/resolutions of the same file need not accumulate.
        self.entries.retain(|(k, p)| {
            if k.path == key.path {
                self.bytes -= p.bytes;
                false
            } else {
                true
            }
        });
        while self.bytes + pixels.bytes > self.limit {
            let Some((_, removed)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= removed.bytes;
        }
        self.bytes += pixels.bytes;
        self.entries.push_back((key, pixels));
    }
}
struct Job {
    key: Key,
    generation: u64,
    foreground: bool,
}
struct State {
    generation: u64,
    pending: Option<Job>,
    result: Option<Completion>,
    closed: bool,
    cache: Cache,
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
        Self::start(CACHE_BYTES, prepare)
    }
}
impl Loader {
    fn start(
        limit: usize,
        prepare: impl Fn(&Key, &dyn Fn() -> bool) -> ResultPixels + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                generation: 0,
                pending: None,
                result: None,
                closed: false,
                cache: Cache {
                    entries: VecDeque::new(),
                    bytes: 0,
                    limit,
                },
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
                let cached = state.cache.get(&job.key);
                drop(state);
                let cancelled = || {
                    let state = worker.state.lock().unwrap();
                    state.closed
                        || (state.generation != job.generation
                            && !state.pending.as_ref().is_some_and(|p| p.key == job.key))
                };
                let result = cached.map_or_else(|| prepare(&job.key, &cancelled), Ok);
                let mut state = worker.state.lock().unwrap();
                if state.closed {
                    break;
                }
                if let Ok(pixels) = &result {
                    state.cache.insert(job.key.clone(), pixels.clone());
                }
                if job.foreground && job.generation == state.generation {
                    state.result = Some((job.key, result));
                }
            }
        });
        Self { shared }
    }
    /// A cache hit is available immediately. Otherwise replace any queued request.
    pub fn request(&self, key: Key) -> Option<Pixels> {
        let mut state = self.shared.state.lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        state.result = None;
        state.pending = None;
        let pixels = state.cache.get(&key);
        if pixels.is_none() {
            state.pending = Some(Job {
                key,
                generation: state.generation,
                foreground: true,
            });
            self.shared.wake.notify_one();
        }
        pixels
    }
    pub fn cancel(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        state.pending = None;
        state.result = None;
    }
    /// Does not supersede foreground work and never publishes a UI completion.
    pub fn prefetch(&self, key: Key) {
        let mut state = self.shared.state.lock().unwrap();
        if state.pending.is_none() && state.cache.get(&key).is_none() {
            state.pending = Some(Job {
                key,
                generation: state.generation,
                foreground: false,
            });
            self.shared.wake.notify_one();
        }
    }
    pub fn take_result(&self) -> Option<Completion> {
        self.shared.state.lock().unwrap().result.take()
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        state.pending = None;
        self.shared.wake.notify_one();
    }
}
fn prepare(key: &Key, cancelled: &dyn Fn() -> bool) -> ResultPixels {
    let started = Instant::now();
    if key.screens.is_empty() || cancelled() {
        return Err("wallpaper request superseded or has no screens".into());
    }
    let mut bytes = 0usize;
    for (index, &(width, height)) in key.screens.iter().enumerate() {
        if width > 16384 || height > 16384 {
            return Err("wallpaper screen dimension exceeds 16384".into());
        }
        if key.screens[..index].contains(&(width, height)) {
            continue;
        }
        let size = u64::from(width) * u64::from(height) * 4;
        if width == 0
            || height == 0
            || size > MAX_PREPARED_BYTES as u64
            || bytes as u64 + size > MAX_PREPARED_BYTES as u64
        {
            return Err("wallpaper display buffers exceed 256 MiB".into());
        }
        bytes += size as usize;
    }
    let pixels = winarchy_theme::pack::decode(&key.path)?;
    let decoded = started.elapsed();
    let mut frames: Vec<Arc<Frame>> = Vec::new();
    for (index, &(width, height)) in key.screens.iter().enumerate() {
        if cancelled() {
            return Err("wallpaper request superseded".into());
        }
        if let Some(previous) = key.screens[..index]
            .iter()
            .position(|s| *s == (width, height))
        {
            frames.push(frames[previous].clone());
            continue;
        }
        let fitted = winarchy_theme::pack::cover(&pixels, width, height)?;
        frames.push(Arc::new(Frame {
            width,
            height,
            pixels: fitted.into_raw(),
        }));
    }
    tracing::info!(
        decode_ms = decoded.as_millis(),
        fit_ms = (started.elapsed() - decoded).as_millis(),
        total_ms = started.elapsed().as_millis(),
        "wallpaper prepared in worker"
    );
    Ok(Arc::new(Prepared { frames, bytes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    fn key(name: &str) -> Key {
        Key {
            path: name.into(),
            size: 10,
            modified: 1,
            screens: vec![(2, 2)],
        }
    }
    fn pixels() -> Pixels {
        Arc::new(Prepared {
            frames: vec![],
            bytes: 16,
        })
    }
    fn wait(loader: &Loader) -> Completion {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = loader.take_result() {
                return result;
            }
            assert!(Instant::now() < end, "worker did not complete");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn cache_is_bounded_lru_and_invalidates_file_versions() {
        let mut cache = Cache {
            entries: VecDeque::new(),
            bytes: 0,
            limit: 32,
        };
        cache.insert(key("a"), pixels());
        cache.insert(key("b"), pixels());
        assert!(cache.get(&key("a")).is_some());
        cache.insert(key("c"), pixels());
        assert!(cache.get(&key("b")).is_none());
        let mut changed = key("a");
        changed.modified = 2;
        assert!(cache.get(&changed).is_none());
        cache.insert(changed.clone(), pixels());
        assert!(cache.get(&key("a")).is_none());
        assert!(cache.get(&changed).is_some());
        assert_eq!(cache.bytes, 32);
        let mut resized = changed.clone();
        resized.screens = vec![(4, 4)];
        assert!(cache.get(&resized).is_none());
        cache.insert(
            key("huge"),
            Arc::new(Prepared {
                frames: vec![],
                bytes: 64,
            }),
        );
        assert_eq!(cache.bytes, 32);
    }
    #[test]
    fn latest_request_wins_without_queuing_every_click() {
        let (started, receive) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let loader = Loader::start(64, move |key, _| {
            started.send(key.path.clone()).unwrap();
            if key.path == std::path::Path::new("a") {
                gate.recv().unwrap();
            }
            Ok(pixels())
        });
        loader.request(key("a"));
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(3)).unwrap(),
            PathBuf::from("a")
        );
        loader.request(key("b"));
        loader.request(key("c"));
        release.send(()).unwrap();
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(3)).unwrap(),
            PathBuf::from("c")
        );
        assert_eq!(wait(&loader).0, key("c"));
        assert!(loader.request(key("c")).is_some());
        assert!(loader.take_result().is_none());
    }
    #[test]
    fn cancellation_drops_inflight_completion_without_losing_useful_cache() {
        let (started, receive) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let loader = Loader::start(64, move |_, _| {
            started.send(()).unwrap();
            gate.recv().unwrap();
            Ok(pixels())
        });
        loader.request(key("a"));
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
        loader.cancel();
        release.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while loader
            .shared
            .state
            .lock()
            .unwrap()
            .cache
            .get(&key("a"))
            .is_none()
        {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(loader.take_result().is_none());
        assert!(loader.request(key("a")).is_some());
    }
    #[test]
    fn prefetched_pixels_are_reused_by_foreground_request() {
        let (started, receive) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let loader = Loader::start(64, move |_, _| {
            started.send(()).unwrap();
            gate.recv().unwrap();
            Ok(pixels())
        });
        loader.prefetch(key("a"));
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
        loader.request(key("a"));
        release.send(()).unwrap();
        assert_eq!(wait(&loader).0, key("a"));
        assert!(receive.try_recv().is_err()); // one decode, not two
        loader.cancel();
        assert!(loader.take_result().is_none());
    }
    #[test]
    fn tiny_images_prepare_once_for_identical_monitor_sizes() {
        let mut key = key("unused");
        key.path = std::env::temp_dir().join(format!("winarchy-wallpaper-fixture-{}.png", std::process::id()));
        // Cross-compiled tests cannot access the build host's manifest path.
        std::fs::write(&key.path, include_bytes!("../../tests/fixtures/wallpaper.png")).unwrap();
        key.screens = vec![(640, 360), (640, 360), (360, 640)];
        let result = prepare(&key, &|| false);
        std::fs::remove_file(&key.path).unwrap();
        let result = result.unwrap();
        assert!(Arc::ptr_eq(&result.frames[0], &result.frames[1]));
        assert_eq!(result.bytes, 640 * 360 * 4 * 2);
        assert_eq!(result.frames[2].pixels.len(), 640 * 360 * 4);
    }
}
