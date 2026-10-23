//! Live physical readers belong to exactly one exclusive State owner.
//!
//! Only that owner's serialized worker admits snapshots or inspects this map.
//! Clones may drop anywhere, but cannot resurrect a zero-strong-count token.
//! Lost replies drop their token automatically. No queue slot or async release
//! is needed, and a logical reader never holds a SQLite transaction open.
use crate::journal::GenerationId;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

pub(crate) struct Pin {
    generation: GenerationId,
}
impl Pin {
    pub(crate) fn generation(&self) -> GenerationId {
        self.generation
    }
}
#[derive(Default)]
pub(crate) struct Readers {
    entries: BTreeMap<i64, Weak<Pin>>,
}
impl Readers {
    /// Call only after the exact generation and sealed manifest are validated
    /// in the owner command. Repeated acquisition shares one retention entry.
    pub(crate) fn acquire(&mut self, generation: GenerationId) -> Arc<Pin> {
        self.entries.retain(|_, pin| pin.strong_count() != 0);
        if let Some(pin) = self
            .entries
            .get(&generation.value())
            .and_then(Weak::upgrade)
        {
            return pin;
        }
        let pin = Arc::new(Pin { generation });
        self.entries
            .insert(generation.value(), Arc::downgrade(&pin));
        pin
    }
    /// Conservative stable protection for this serialized owner command.
    /// Concurrent drops may overprotect; new acquisition cannot race the command.
    pub(crate) fn active(&mut self) -> Vec<GenerationId> {
        self.entries.retain(|_, pin| pin.strong_count() != 0);
        self.entries
            .values()
            .filter_map(Weak::upgrade)
            .map(|pin| pin.generation)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_readers_clones_share_one_entry_and_expired_entries_are_bounded() {
        let mut readers = Readers::default();
        let first = GenerationId::try_from(1).unwrap();
        let original = readers.acquire(first);
        let clone = original.clone();
        let separate = readers.acquire(first);
        assert!(Arc::ptr_eq(&original, &separate));
        drop(original);
        drop(separate);
        assert_eq!(readers.active(), vec![first]);
        for id in 2..4098 {
            let transient = readers.acquire(GenerationId::try_from(id).unwrap());
            assert_eq!(readers.entries.len(), 2);
            drop(transient);
        }
        assert_eq!(readers.active(), vec![first]);
        drop(clone);
        assert!(readers.active().is_empty());
        assert!(readers.entries.is_empty());
    }
    #[test]
    fn physical_readers_old_owner_tokens_cannot_pin_replacement_owner() {
        let first = GenerationId::try_from(1).unwrap();
        let mut old = Readers::default();
        let original = old.acquire(first);
        drop(old);
        let mut replacement = Readers::default();
        assert!(replacement.active().is_empty());
        let new = replacement.acquire(first);
        assert!(!Arc::ptr_eq(&original, &new));
        drop(original);
        assert_eq!(replacement.active(), vec![first]);
    }
}
