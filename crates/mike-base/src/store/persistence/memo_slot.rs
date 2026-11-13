//! Per-handle memos whose types the features own. The handle cannot name a
//! feature's memo type (it sits below every feature in the crate split), so it
//! keeps them in a slot keyed by type, created on first use and shared by
//! every clone of the handle.

use std::any::{Any, TypeId};
use std::sync::Arc;

use dashmap::DashMap;

use super::PersistHandle;

pub type MemoSlot = Arc<DashMap<TypeId, Arc<dyn Any + Send + Sync>>>;

impl PersistHandle {
    pub fn memo<T: Default + Send + Sync + 'static>(&self) -> Arc<T> {
        let entry = self
            .memos
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(T::default()) as Arc<dyn Any + Send + Sync>)
            .clone();
        match entry.downcast::<T>() {
            Ok(memo) => memo,
            Err(_) => {
                let fresh = Arc::new(T::default());
                self.memos.insert(TypeId::of::<T>(), fresh.clone());
                fresh
            }
        }
    }
}
