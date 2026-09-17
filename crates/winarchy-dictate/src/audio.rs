//! Default-microphone capture through WASAPI in shared mode. The stream is
//! requested as 16 kHz mono float and Windows converts the endpoint's mix
//! format, so the samples feed the model as they are.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use windows::Win32::{
    Foundation::CloseHandle,
    Media::Audio::*,
    System::{
        Com::*,
        Threading::{CreateEventW, WaitForSingleObject},
    },
};
pub const SAMPLE_RATE: u32 = 16_000;
/// Longer holds keep the first minute: the model is meant for dictation, not meetings.
const MAX_SAMPLES: usize = 60 * SAMPLE_RATE as usize;
pub struct Capture {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Capture {
    /// Opens the default input and records until [`Capture::finish`]; `level`
    /// receives the RMS of each packet in 0..=1 for the indicator.
    pub fn start(level: impl Fn(f32) + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::with_capacity(SAMPLE_RATE as usize * 10)));
        let (ready, opened) = mpsc::channel();
        let worker = {
            let stop = stop.clone();
            let samples = samples.clone();
            std::thread::spawn(move || unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                match open() {
                    Ok(stream) => {
                        let _ = ready.send(Ok(()));
                        stream.run(&stop, &samples, level);
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                    }
                }
                CoUninitialize();
            })
        };
        opened
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|_| "microphone did not open in time".to_owned())??;
        Ok(Self {
            stop,
            samples,
            worker: Some(worker),
        })
    }
    /// Stops recording and returns everything captured.
    pub fn finish(mut self) -> Vec<f32> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()))
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: windows::Win32::Foundation::HANDLE,
}
unsafe fn open() -> Result<Stream, String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .map_err(|_| "no default microphone".to_owned())?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("microphone unavailable: {e}"))?;
        // WAVE_FORMAT_IEEE_FLOAT, mono, 16 kHz: the model's input format.
        let format = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 1,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 4,
            nBlockAlign: 4,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                // 100 ms of buffering in 100 ns units.
                1_000_000,
                0,
                &format,
                None,
            )
            .map_err(|e| format!("microphone format refused: {e}"))?;
        let event = CreateEventW(None, false, false, None).map_err(|e| e.to_string())?;
        client.SetEventHandle(event).map_err(|e| e.to_string())?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| e.to_string())?;
        client.Start().map_err(|e| e.to_string())?;
        Ok(Stream {
            client,
            capture,
            event,
        })
    }
}
impl Stream {
    unsafe fn run(self, stop: &AtomicBool, samples: &Mutex<Vec<f32>>, level: impl Fn(f32)) {
        unsafe {
            while !stop.load(Ordering::Acquire) {
                let _ = WaitForSingleObject(self.event, 200);
                loop {
                    let Ok(packet) = self.capture.GetNextPacketSize() else {
                        break;
                    };
                    if packet == 0 {
                        break;
                    }
                    let mut data: *mut u8 = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    if self
                        .capture
                        .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                        .is_err()
                    {
                        break;
                    }
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                    let chunk: Vec<f32> = if silent || data.is_null() {
                        vec![0.0; frames as usize]
                    } else {
                        std::slice::from_raw_parts(data as *const f32, frames as usize).to_vec()
                    };
                    let _ = self.capture.ReleaseBuffer(frames);
                    let rms = (chunk.iter().map(|s| s * s).sum::<f32>()
                        / chunk.len().max(1) as f32)
                        .sqrt();
                    level(rms.min(1.0));
                    let mut store = samples.lock().unwrap_or_else(|e| e.into_inner());
                    let room = MAX_SAMPLES.saturating_sub(store.len());
                    store.extend_from_slice(&chunk[..chunk.len().min(room)]);
                }
            }
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}
