//! The runtime crate's queries on the shared database handle: `persist.runtime()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! runtime crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The runtime crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct RuntimeDb<'a>(&'a PersistHandle);

impl Deref for RuntimeDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.runtime()` on the handle.
pub trait RuntimePersist {
    fn runtime(&self) -> RuntimeDb<'_>;
}

impl RuntimePersist for PersistHandle {
    fn runtime(&self) -> RuntimeDb<'_> {
        RuntimeDb(self)
    }
}
