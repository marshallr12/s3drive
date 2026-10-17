//! Linux FUSE adapter for the shared filesystem engine.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::run;

pub const IMPLEMENTATION_STATUS: &str = "Linux FUSE with optional SQLite cache; explicit writes and guarded file creation with native acceptance";
