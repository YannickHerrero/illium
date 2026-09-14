pub use winarchy_ipc::{command, protocol};
pub mod applets;
pub mod clock;
pub mod config;
pub mod files;
pub mod keyboard;
pub mod layout;
pub mod model;
pub mod modifiers;
#[cfg(windows)]
pub mod platform;
pub mod queue;
pub mod request;
