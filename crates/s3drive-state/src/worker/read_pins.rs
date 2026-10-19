//! Pending local pin releases, independent of command queue capacity.
use super::*;
use crate::sparse_views::{self, pins::Pin};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Default)]
pub(super) struct Pending {
    wake_queued: AtomicBool,
    entries: Mutex<BTreeMap<sparse_views::Id, (Pin, sparse_views::Options)>>,
}
impl Pending {
    pub(super) fn woke(&self) {
        self.wake_queued.store(false, Ordering::Release);
    }
    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, BTreeMap<sparse_views::Id, (Pin, sparse_views::Options)>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    pub(super) fn drain(&self, store: &mut cache::Store) {
        // At most MAX_ACTIVE actual pins; SQL never holds this mutex. Failed
        // entries remain conservative retention, retried only on later work.
        let entries: Vec<_> = self.lock().values().cloned().collect();
        for (pin, options) in entries {
            let released = sparse_views::Journal::new(&mut store.state, options)
                .and_then(|mut journal| journal.release_pin(&pin));
            if released.is_ok() {
                self.lock().remove(&pin.id());
            }
        }
    }
}
impl Owner {
    pub(crate) fn release_read_pin(&self, pin: Pin, options: sparse_views::Options) {
        // Tokens are private to guards; no explicit early release can leave a
        // stale entry while allowing unbounded new guarded admissions.
        self.read_pins.lock().insert(pin.id(), (pin, options));
        // If full, already accepted work drains the map. If closed, durable
        // retention is retired only by the next exclusive owner's incarnation.
        if !self.read_pins.wake_queued.swap(true, Ordering::AcqRel)
            && self.sender.try_send(Command::WakeReadPins).is_err()
        {
            self.read_pins.wake_queued.store(false, Ordering::Release);
        }
    }
}
#[cfg(test)]
mod tests;
