//! Linux adapter boundary. The fuser dependency and callbacks arrive in P2.

pub const IMPLEMENTATION_STATUS: &str = "planned: Linux FUSE adapter; no mount implementation";

/// Fail explicitly until real callbacks and unmount handling are verified.
pub fn mount() -> Result<(), &'static str> {
    Err(IMPLEMENTATION_STATUS)
}
