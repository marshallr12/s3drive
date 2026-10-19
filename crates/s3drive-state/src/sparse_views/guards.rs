//! Clone-sharing local retention. No cloud authorization or reclamation.
use super::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct ReadGuard(Arc<Inner>);
struct Inner {
    client: Client,
    view: View,
    pin: pins::Pin,
}
impl fmt::Debug for ReadGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceReadGuard([private])")
    }
}
impl ReadGuard {
    pub(super) fn new(client: Client, view: View, pin: pins::Pin) -> Self {
        Self(Arc::new(Inner { client, view, pin }))
    }
    pub fn view(&self) -> &View {
        &self.0.view
    }
    /// Local pieces only. The owning guard remains alive throughout the call.
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<Piece>> {
        self.0
            .client
            .read_pinned(self.0.pin.clone(), offset, length)
            .await
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.client.release_guard(self.pin.clone());
    }
}

impl std::ops::Deref for ReadGuard {
    type Target = View;
    fn deref(&self) -> &View {
        self.view()
    }
}
