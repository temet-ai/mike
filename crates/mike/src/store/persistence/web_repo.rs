use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct WebDb<'a>(&'a PersistHandle);

impl Deref for WebDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait WebPersist {
    fn web(&self) -> WebDb<'_>;
}

impl WebPersist for PersistHandle {
    fn web(&self) -> WebDb<'_> {
        WebDb(self)
    }
}
