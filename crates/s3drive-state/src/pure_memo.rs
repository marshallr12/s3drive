//! Worker-lifetime memo of pure representations, never validation outcomes.
//! The single retained allocation is an exact-size canonical byte slice. Values
//! are fixed-size payload-reference bytes. No decoded object graph is retained.
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

pub(crate) const MAX_RAW: usize = 16 * 1024;
// Retained storage is at most MAX_RAW plus size_of::<RefCell<Memo>>():
// one exact-size boxed slice and inline fixed arrays, no nested allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct References(pub(crate) [Option<[u8; 64]>; 3]);
impl References {
    pub(crate) fn text(&self, index: usize) -> crate::Result<Option<&str>> {
        self.0[index]
            .as_ref()
            .map(|v| std::str::from_utf8(v).map_err(Into::into))
            .transpose()
    }
}
struct Entry {
    raw: Box<[u8]>,
    references: References,
}
#[derive(Default)]
struct Memo {
    active: bool,
    entry: Option<Entry>,
}
thread_local! { static MEMO: RefCell<Memo> = RefCell::new(Memo::default()); }

/// Not Send: activation and cleanup belong to the same dedicated owner thread.
pub(crate) struct Lifetime(PhantomData<Rc<()>>);
impl Lifetime {
    pub(crate) fn begin() -> crate::Result<Self> {
        MEMO.with(|cell| {
            let mut memo = cell
                .try_borrow_mut()
                .map_err(|_| "representation memo is borrowed")?;
            if memo.active {
                return Err("representation memo is already active".into());
            }
            memo.entry = None;
            memo.active = true;
            Ok(Self(PhantomData))
        })
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        MEMO.with(|cell| *cell.borrow_mut() = Memo::default());
    }
}
pub(crate) fn get(raw: &[u8]) -> Option<References> {
    let result = MEMO.with(|cell| {
        let memo = cell.borrow();
        memo.active.then_some(())?;
        let entry = memo.entry.as_ref()?;
        (entry.raw.as_ref() == raw).then_some(entry.references)
    });
    #[cfg(test)]
    WORK.with(|work| {
        let mut value = work.get();
        value[usize::from(result.is_some())] += 1;
        work.set(value);
    });
    result
}
/// Call only with bytes just checked equal to their canonical serialization.
/// This remembers data, not the result of any semantic or database validation.
pub(crate) fn remember(raw: &[u8], references: References) {
    MEMO.with(|cell| {
        let mut memo = cell.borrow_mut();
        if memo.active && raw.len() <= MAX_RAW {
            memo.entry = Some(Entry {
                raw: raw.into(),
                references,
            });
        }
    });
}

#[cfg(test)]
thread_local! { static WORK: std::cell::Cell<[u64;2]> = const { std::cell::Cell::new([0;2]) }; }
#[cfg(test)]
pub(crate) fn work() -> [u64; 2] {
    WORK.with(std::cell::Cell::get)
}
#[cfg(test)]
pub(crate) fn clear_entry() {
    MEMO.with(|cell| cell.borrow_mut().entry = None);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn representation_memo_is_bounded_exact_and_worker_lifetime_only() {
        let values = References([Some([b'a'; 64]), None, None]);
        remember(b"one", values);
        assert!(get(b"one").is_none());
        {
            let _lifetime = Lifetime::begin().unwrap();
            assert!(Lifetime::begin().is_err());
            remember(b"one", values);
            assert_eq!(get(b"one"), Some(values));
            assert!(get(b"two").is_none());
            std::thread::spawn(move || {
                assert!(get(b"one").is_none());
                remember(b"one", values);
                assert!(get(b"one").is_none());
            })
            .join()
            .unwrap();
            remember(b"two", values);
            assert!(get(b"one").is_none());
            let limit = vec![0; MAX_RAW];
            remember(&limit, values);
            MEMO.with(|cell| {
                let memo = cell.borrow();
                let entry = memo.entry.as_ref().unwrap();
                // Box<[u8]> has no spare capacity; all other retained fields
                // are inline and included in this explicit bound.
                assert_eq!(entry.raw.len(), MAX_RAW);
                assert!(entry.raw.len() + size_of::<Memo>() <= MAX_RAW + size_of::<Memo>());
            });
            remember(&vec![0; MAX_RAW + 1], values);
            assert_eq!(get(&limit), Some(values));
        }
        assert!(get(b"two").is_none());
        let _ = std::panic::catch_unwind(|| {
            let _lifetime = Lifetime::begin().unwrap();
            remember(b"panic", values);
            panic!("test cleanup");
        });
        let _lifetime = Lifetime::begin().unwrap();
        assert!(get(b"panic").is_none());
    }
}
