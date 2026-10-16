//! Windows adapter boundary. WinFsp binding/licensing selection is still open.

pub const IMPLEMENTATION_STATUS: &str = "planned: Windows WinFsp adapter; no mount implementation";

pub fn mount() -> Result<(), &'static str> {
    Err(IMPLEMENTATION_STATUS)
}
