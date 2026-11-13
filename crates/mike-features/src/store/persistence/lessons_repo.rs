//! The lessons crate's queries on the shared database handle: `persist.lessons()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! lessons crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The lessons crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct LessonsDb<'a>(&'a PersistHandle);

impl Deref for LessonsDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.lessons()` on the handle.
pub trait LessonsPersist {
    fn lessons(&self) -> LessonsDb<'_>;
}

impl LessonsPersist for PersistHandle {
    fn lessons(&self) -> LessonsDb<'_> {
        LessonsDb(self)
    }
}
