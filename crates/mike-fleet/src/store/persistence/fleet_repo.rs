use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct FleetDb<'a>(&'a PersistHandle);

impl Deref for FleetDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait FleetPersist {
    fn fleet(&self) -> FleetDb<'_>;
}

impl FleetPersist for PersistHandle {
    fn fleet(&self) -> FleetDb<'_> {
        FleetDb(self)
    }
}
