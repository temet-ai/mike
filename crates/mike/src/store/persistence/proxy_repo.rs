//! The proxy crate's queries on the shared database handle: `persist.proxy()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! proxy crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The proxy crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct ProxyDb<'a>(&'a PersistHandle);

impl Deref for ProxyDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.proxy()` on the handle.
pub trait ProxyPersist {
    fn proxy(&self) -> ProxyDb<'_>;
}

impl ProxyPersist for PersistHandle {
    fn proxy(&self) -> ProxyDb<'_> {
        ProxyDb(self)
    }
}
