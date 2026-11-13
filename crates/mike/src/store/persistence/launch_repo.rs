//! The launch crate's queries on the shared database handle: `persist.launch()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! launch crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The launch crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct LaunchDb<'a>(&'a PersistHandle);

impl Deref for LaunchDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.launch()` on the handle.
pub trait LaunchPersist {
    fn launch(&self) -> LaunchDb<'_>;
}

impl LaunchPersist for PersistHandle {
    fn launch(&self) -> LaunchDb<'_> {
        LaunchDb(self)
    }
}
