//! The steerer crate's queries on the shared database handle: `persist.steerer()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! steerer crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The steerer crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct SteererDb<'a>(&'a PersistHandle);

impl Deref for SteererDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.steerer()` on the handle.
pub trait SteererPersist {
    fn steerer(&self) -> SteererDb<'_>;
}

impl SteererPersist for PersistHandle {
    fn steerer(&self) -> SteererDb<'_> {
        SteererDb(self)
    }
}
