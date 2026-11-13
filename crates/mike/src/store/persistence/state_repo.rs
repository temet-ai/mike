//! The state crate's queries on the shared database handle: `persist.state()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! state crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The state crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct StateDb<'a>(&'a PersistHandle);

impl Deref for StateDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.state()` on the handle.
pub trait StatePersist {
    fn state(&self) -> StateDb<'_>;
}

impl StatePersist for PersistHandle {
    fn state(&self) -> StateDb<'_> {
        StateDb(self)
    }
}
