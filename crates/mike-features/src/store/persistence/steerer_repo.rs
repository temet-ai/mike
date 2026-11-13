use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct SteererDb<'a>(&'a PersistHandle);

impl Deref for SteererDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait SteererPersist {
    fn steerer(&self) -> SteererDb<'_>;
}

impl SteererPersist for PersistHandle {
    fn steerer(&self) -> SteererDb<'_> {
        SteererDb(self)
    }
}
