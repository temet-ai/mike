use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct DossierDb<'a>(&'a PersistHandle);

impl Deref for DossierDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait DossierPersist {
    fn dossier(&self) -> DossierDb<'_>;
}

impl DossierPersist for PersistHandle {
    fn dossier(&self) -> DossierDb<'_> {
        DossierDb(self)
    }
}
