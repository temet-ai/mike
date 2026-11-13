use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct ProxyDb<'a>(&'a PersistHandle);

impl Deref for ProxyDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait ProxyPersist {
    fn proxy(&self) -> ProxyDb<'_>;
}

impl ProxyPersist for PersistHandle {
    fn proxy(&self) -> ProxyDb<'_> {
        ProxyDb(self)
    }
}
