//! Linux FUSE adapter for the shared read-only filesystem.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::run;

pub const IMPLEMENTATION_STATUS: &str =
    "read-only Linux FUSE adapter; no writable mount or disk cache";
