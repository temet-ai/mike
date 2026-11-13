use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct LessonsDb<'a>(&'a PersistHandle);

impl Deref for LessonsDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait LessonsPersist {
    fn lessons(&self) -> LessonsDb<'_>;
}

impl LessonsPersist for PersistHandle {
    fn lessons(&self) -> LessonsDb<'_> {
        LessonsDb(self)
    }
}
