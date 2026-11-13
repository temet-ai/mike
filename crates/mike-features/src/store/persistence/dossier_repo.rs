//! The dossier crate's queries on the shared database handle: `persist.dossier()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! dossier crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The dossier crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct DossierDb<'a>(&'a PersistHandle);

impl Deref for DossierDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.dossier()` on the handle.
pub trait DossierPersist {
    fn dossier(&self) -> DossierDb<'_>;
}

impl DossierPersist for PersistHandle {
    fn dossier(&self) -> DossierDb<'_> {
        DossierDb(self)
    }
}
