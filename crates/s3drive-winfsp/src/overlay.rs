//! Per-mount, in-memory file attributes and times.
//!
//! The managed layout stores neither Windows attributes nor timestamps yet.
//! Applications such as Office and `CopyFile` still set and read them, so the
//! adapter remembers them for the lifetime of one mount. Nothing here is
//! written locally or remotely; a remount forgets every entry.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// `FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_TEMPORARY`.
pub(crate) const REMEMBERED: u32 = 0x2 | 0x4 | 0x100;
/// Remembered bits plus the storage-neutral ARCHIVE, NORMAL and
/// NOT_CONTENT_INDEXED bits, which are accepted and then ignored.
pub(crate) const ACCEPTED: u32 = REMEMBERED | 0x20 | 0x80 | 0x2000;
/// Upper bound on remembered nodes; later nodes simply keep default values.
const MAX_ENTRIES: usize = 65_536;
/// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
const EPOCH_GAP_SECONDS: u64 = 11_644_473_600;

/// Remembered values for one node. A zero time means "not recorded".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub attributes: u32,
    pub creation: u64,
    pub access: u64,
    pub write: u64,
    pub change: u64,
}

/// One requested basic-information change. `None` and zero leave a field alone.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Change {
    pub attributes: Option<u32>,
    pub creation: u64,
    pub access: u64,
    pub write: u64,
    pub change: u64,
}

/// Bounded map from filesystem node ID to its remembered values.
#[derive(Default)]
pub(crate) struct Overlay {
    entries: Mutex<HashMap<u64, Stamp>>,
}

/// Current time as a Windows FILETIME (100 ns ticks since 1601-01-01 UTC).
pub(crate) fn now() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (elapsed.as_secs() + EPOCH_GAP_SECONDS) * 10_000_000 + u64::from(elapsed.subsec_nanos()) / 100
}

/// Returns `new` unless it is the "leave unchanged" value zero.
fn chosen(old: u64, new: u64) -> u64 {
    if new == 0 { old } else { new }
}

impl Overlay {
    /// Runs `update` on the node's entry, creating it while capacity remains.
    fn update(&self, node: u64, update: impl FnOnce(&mut Stamp)) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() < MAX_ENTRIES || entries.contains_key(&node) {
            update(entries.entry(node).or_default());
        }
    }
    /// Records a new node's create-time attributes and its initial times.
    pub(crate) fn created(&self, node: u64, attributes: u32, time: u64) {
        self.update(node, |stamp| {
            *stamp = Stamp {
                attributes: attributes & REMEMBERED,
                creation: time,
                access: time,
                write: time,
                change: time,
            }
        });
    }
    /// Records that the node's content or size changed at `time`.
    pub(crate) fn written(&self, node: u64, time: u64) {
        self.update(node, |stamp| {
            stamp.write = time;
            stamp.change = time;
        });
    }
    /// Records an overwrite's attributes: replaced on supersede, else added.
    pub(crate) fn overwritten(&self, node: u64, attributes: u32, replace: bool) {
        self.update(node, |stamp| {
            let kept = if replace { 0 } else { stamp.attributes };
            stamp.attributes = kept | (attributes & REMEMBERED);
        });
    }
    /// Applies an application's explicit attribute and time change.
    pub(crate) fn set(&self, node: u64, change: Change) {
        self.update(node, |stamp| {
            if let Some(attributes) = change.attributes {
                stamp.attributes = attributes & REMEMBERED;
            }
            stamp.creation = chosen(stamp.creation, change.creation);
            stamp.access = chosen(stamp.access, change.access);
            stamp.write = chosen(stamp.write, change.write);
            stamp.change = chosen(stamp.change, change.change);
        });
    }
    /// Returns the remembered values for a node, if any were recorded.
    pub(crate) fn get(&self, node: u64) -> Option<Stamp> {
        self.entries.lock().ok()?.get(&node).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The accepted set is the remembered bits plus the three ignored neutral
    /// bits; READONLY, COMPRESSED and ENCRYPTED stay outside it. Only the
    /// Windows adapter reads `ACCEPTED`, so this is also its portable check.
    #[test]
    fn accepted_attributes_cover_remembered_and_neutral_bits_only() {
        assert_eq!(ACCEPTED & REMEMBERED, REMEMBERED);
        assert_eq!(ACCEPTED & !REMEMBERED, 0x20 | 0x80 | 0x2000);
        for refused in [0x1, 0x800, 0x4000] {
            assert_eq!(ACCEPTED & refused, 0);
        }
    }

    #[test]
    fn creation_keeps_only_remembered_attributes_and_stamps_all_times() {
        let overlay = Overlay::default();
        overlay.created(7, 0x2 | 0x20 | 0x100 | 0x2000, 42);
        let stamp = overlay.get(7).unwrap();
        assert_eq!(stamp.attributes, 0x2 | 0x100);
        assert_eq!(
            (stamp.creation, stamp.access, stamp.write, stamp.change),
            (42, 42, 42, 42)
        );
        assert!(overlay.get(8).is_none());
    }

    #[test]
    fn writes_advance_write_and_change_times_only() {
        let overlay = Overlay::default();
        overlay.created(1, 0, 10);
        overlay.written(1, 20);
        let stamp = overlay.get(1).unwrap();
        assert_eq!(
            (stamp.creation, stamp.access, stamp.write, stamp.change),
            (10, 10, 20, 20)
        );
    }

    #[test]
    fn explicit_change_leaves_absent_attributes_and_zero_times_alone() {
        let overlay = Overlay::default();
        overlay.created(1, 0x2, 10);
        overlay.set(
            1,
            Change {
                write: 99,
                ..Change::default()
            },
        );
        let stamp = overlay.get(1).unwrap();
        assert_eq!(
            (stamp.attributes, stamp.creation, stamp.write),
            (0x2, 10, 99)
        );
        overlay.set(
            1,
            Change {
                attributes: Some(0x80),
                ..Change::default()
            },
        );
        assert_eq!(overlay.get(1).unwrap().attributes, 0);
    }

    #[test]
    fn overwrite_adds_attributes_and_supersede_replaces_them() {
        let overlay = Overlay::default();
        overlay.created(1, 0x2, 10);
        overlay.overwritten(1, 0x100 | 0x20, false);
        assert_eq!(overlay.get(1).unwrap().attributes, 0x2 | 0x100);
        overlay.overwritten(1, 0x4, true);
        assert_eq!(overlay.get(1).unwrap().attributes, 0x4);
    }

    #[test]
    fn capacity_bound_ignores_new_nodes_but_updates_known_ones() {
        let overlay = Overlay::default();
        for node in 0..MAX_ENTRIES as u64 {
            overlay.written(node, 1);
        }
        overlay.written(u64::MAX, 5);
        assert!(overlay.get(u64::MAX).is_none());
        overlay.written(0, 5);
        assert_eq!(overlay.get(0).unwrap().write, 5);
    }

    #[test]
    fn now_is_after_2020_in_filetime_ticks() {
        // 2020-01-01T00:00:00Z as FILETIME.
        assert!(now() > 132_223_104_000_000_000);
    }
}
