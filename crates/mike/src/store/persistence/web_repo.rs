//! The web crate's queries on the shared database handle: `persist.web()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! web crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The web crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct WebDb<'a>(&'a PersistHandle);

impl Deref for WebDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.web()` on the handle.
pub trait WebPersist {
    fn web(&self) -> WebDb<'_>;
}

impl WebPersist for PersistHandle {
    fn web(&self) -> WebDb<'_> {
        WebDb(self)
    }
}
