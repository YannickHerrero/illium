//! Shared command schema and bounded Windows IPC, independent of the desktop UI.
#[cfg(windows)]
pub mod client;
pub mod command;
#[cfg(windows)]
pub mod identity;
#[cfg(windows)]
pub mod pipe_io;
#[cfg(all(windows, test))]
mod pipe_io_tests;
pub mod protocol;
#[cfg(windows)]
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
