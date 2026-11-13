use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct KnowledgeDb<'a>(&'a PersistHandle);

impl Deref for KnowledgeDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait KnowledgePersist {
    fn knowledge(&self) -> KnowledgeDb<'_>;
}

impl KnowledgePersist for PersistHandle {
    fn knowledge(&self) -> KnowledgeDb<'_> {
        KnowledgeDb(self)
    }
}
