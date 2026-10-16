//! Read-only Windows drive. Native acceptance is separate from portable tests.
#[cfg(any(windows, test))]
mod names;
#[cfg(windows)]
#[allow(unsafe_code)]
mod native;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::run;

pub const IMPLEMENTATION_STATUS: &str =
    "read-only WinFsp adapter; native acceptance pending; no writes or disk cache";
