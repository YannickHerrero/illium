//! Configuration files, applet manifests and local plugin packages. Free of
//! the desktop toolkit so that winarchyctl can manage plugins without linking
//! the daemon.
pub mod applets;
pub mod clock;
pub mod config;
pub mod files;
pub mod keyboard;
pub mod plugins;
