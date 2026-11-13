//! The fleet crate's queries on the shared database handle: `persist.fleet()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! fleet crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The fleet crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct FleetDb<'a>(&'a PersistHandle);

impl Deref for FleetDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.fleet()` on the handle.
pub trait FleetPersist {
    fn fleet(&self) -> FleetDb<'_>;
}

impl FleetPersist for PersistHandle {
    fn fleet(&self) -> FleetDb<'_> {
        FleetDb(self)
    }
}
