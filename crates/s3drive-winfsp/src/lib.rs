//! Windows drive. Native acceptance is separate from portable tests.
#[cfg(any(windows, test))]
mod names;
#[cfg(windows)]
#[allow(unsafe_code)]
mod native;
#[cfg(windows)]
mod windows;
#[cfg(all(windows, feature = "native-tests"))]
pub use native::lifecycle_probe as native_test_api;
#[cfg(windows)]
pub use windows::run;

pub const IMPLEMENTATION_STATUS: &str = "WinFsp with optional SQLite cache; read-only acceptance verified; explicit writes and guarded file creation with native acceptance";
