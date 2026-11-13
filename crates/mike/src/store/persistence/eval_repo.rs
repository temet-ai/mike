//! The eval crate's queries on the shared database handle: `persist.eval()`.
//!
//! A crate cannot add inherent methods to a type another crate defines, so the
//! eval crate's repository methods live on this wrapper instead of on
//! `PersistHandle` (docs/adr/0002-split-crate-mike.md).

use std::ops::Deref;

use super::PersistHandle;

/// The eval crate's view of the handle. Copy, so its async methods take `self`
/// by value and borrow the handle, not this wrapper.
#[derive(Clone, Copy)]
pub struct EvalDb<'a>(&'a PersistHandle);

impl Deref for EvalDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

/// Gives `persist.eval()` on the handle.
pub trait EvalPersist {
    fn eval(&self) -> EvalDb<'_>;
}

impl EvalPersist for PersistHandle {
    fn eval(&self) -> EvalDb<'_> {
        EvalDb(self)
    }
}
