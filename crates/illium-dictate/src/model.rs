//! The one model of this version: Parakeet TDT 0.6B v3, int8, the archive
//! Handy publishes. Fetched on first use with the `curl.exe` and `tar.exe`
//! that ship with Windows, checked against a pinned SHA-256, kept under the
//! user's local application data.
use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};
use transcribe_rs::onnx::{
    Quantization,
    parakeet::{ParakeetModel, ParakeetParams},
};
const DIR: &str = "parakeet-tdt-0.6b-v3-int8";
const URL: &str = "https://blob.handy.computer/parakeet-v3-int8.tar.gz";
const SHA256: &str = "43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77";
pub const SIZE_MB: u32 = 456;
const FILES: [&str; 4] = [
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
fn models_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(base).join("Illium").join("models"))
}
pub fn model_dir() -> Result<PathBuf, String> {
    Ok(models_dir()?.join(DIR))
}
fn complete(dir: &std::path::Path) -> bool {
    FILES.iter().all(|f| dir.join(f).is_file())
}
fn system_tool(name: &str) -> PathBuf {
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(system).join("System32").join(name)
}
fn run(tool: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = Command::new(system_tool(tool))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("{tool}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{tool} exited with {status}"))
    }
}
fn sha256_of(path: &std::path::Path) -> Result<String, String> {
    use sha2::Digest;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}
/// The model directory, downloading and verifying the archive when absent.
pub fn ensure() -> Result<PathBuf, String> {
    let models = models_dir()?;
    let dir = models.join(DIR);
    if complete(&dir) {
        return Ok(dir);
    }
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    let archive = models.join("parakeet-v3-int8.tar.gz");
    let partial = models.join("parakeet-v3-int8.tar.gz.part");
    let _ = std::fs::remove_file(&partial);
    run(
        "curl.exe",
        &[
            "-L".as_ref(),
            "--fail".as_ref(),
            "--silent".as_ref(),
            "--show-error".as_ref(),
            "-o".as_ref(),
            partial.as_os_str(),
            URL.as_ref(),
        ],
    )?;
    if sha256_of(&partial)? != SHA256 {
        let _ = std::fs::remove_file(&partial);
        return Err("model archive checksum mismatch; download aborted".into());
    }
    std::fs::rename(&partial, &archive).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    run(
        "tar.exe",
        &[
            "-xzf".as_ref(),
            archive.as_os_str(),
            "-C".as_ref(),
            models.as_os_str(),
        ],
    )?;
    let _ = std::fs::remove_file(&archive);
    // The archive was built on macOS and carries resource-fork siblings.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("._") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    if complete(&dir) {
        Ok(dir)
    } else {
        Err("model archive did not contain the expected files".into())
    }
}
pub fn load(dir: &std::path::Path) -> Result<ParakeetModel, String> {
    ParakeetModel::load(dir, &Quantization::Int8).map_err(|e| format!("model load failed: {e}"))
}
pub fn transcribe(model: &mut ParakeetModel, samples: &[f32]) -> Result<String, String> {
    model
        .transcribe_with(samples, &ParakeetParams::default())
        .map(|r| r.text.trim().to_owned())
        .map_err(|e| format!("transcription failed: {e}"))
}
