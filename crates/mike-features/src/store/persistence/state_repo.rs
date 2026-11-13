use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct StateDb<'a>(&'a PersistHandle);

impl Deref for StateDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait StatePersist {
    fn state(&self) -> StateDb<'_>;
}

impl StatePersist for PersistHandle {
    fn state(&self) -> StateDb<'_> {
        StateDb(self)
    }
}
