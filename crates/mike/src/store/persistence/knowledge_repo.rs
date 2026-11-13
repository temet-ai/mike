//! The knowledge crate's queries on the shared database handle: `persist.knowledge()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! knowledge crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The knowledge crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct KnowledgeDb<'a>(&'a PersistHandle);

impl Deref for KnowledgeDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.knowledge()` on the handle.
pub trait KnowledgePersist {
    fn knowledge(&self) -> KnowledgeDb<'_>;
}

impl KnowledgePersist for PersistHandle {
    fn knowledge(&self) -> KnowledgeDb<'_> {
        KnowledgeDb(self)
    }
}
