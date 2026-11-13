use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct RuntimeDb<'a>(&'a PersistHandle);

impl Deref for RuntimeDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait RuntimePersist {
    fn runtime(&self) -> RuntimeDb<'_>;
}

impl RuntimePersist for PersistHandle {
    fn runtime(&self) -> RuntimeDb<'_> {
        RuntimeDb(self)
    }
}
