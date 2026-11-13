use std::ops::Deref;

use super::PersistHandle;

#[derive(Clone, Copy)]
pub struct EvalDb<'a>(&'a PersistHandle);

impl Deref for EvalDb<'_> {
    type Target = PersistHandle;

    fn deref(&self) -> &PersistHandle {
        self.0
    }
}

pub trait EvalPersist {
    fn eval(&self) -> EvalDb<'_>;
}

impl EvalPersist for PersistHandle {
    fn eval(&self) -> EvalDb<'_> {
        EvalDb(self)
    }
}
