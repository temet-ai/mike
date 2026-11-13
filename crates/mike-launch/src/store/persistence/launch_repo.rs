use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct LaunchDb<'a>(&'a PersistHandle);

impl Deref for LaunchDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait LaunchPersist {
    fn launch(&self) -> LaunchDb<'_>;
}

impl LaunchPersist for PersistHandle {
    fn launch(&self) -> LaunchDb<'_> {
        LaunchDb(self)
    }
}
