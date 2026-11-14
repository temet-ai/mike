use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

pub trait OverlaySection: Default + Clone + Debug + PartialEq + Send + Sync + 'static {}

trait ErasedOverlay: Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn same_as(&self, other: &dyn ErasedOverlay) -> bool;
    fn is_default(&self) -> bool;
}

impl<T: OverlaySection> ErasedOverlay for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn same_as(&self, other: &dyn ErasedOverlay) -> bool {
        other.as_any().downcast_ref::<T>() == Some(self)
    }

    fn is_default(&self) -> bool {
        *self == T::default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct FeatureOverlays {
    slots: BTreeMap<TypeId, Arc<dyn ErasedOverlay>>,
}

impl FeatureOverlays {
    pub fn get<T: OverlaySection>(&self) -> T {
        self.slots
            .get(&TypeId::of::<T>())
            .and_then(|slot| slot.as_any().downcast_ref::<T>())
            .cloned()
            .unwrap_or_default()
    }

    pub fn update<T: OverlaySection>(&mut self, change: impl FnOnce(&mut T)) {
        let mut value = self.get::<T>();
        change(&mut value);
        self.slots.insert(TypeId::of::<T>(), Arc::new(value));
    }
}

impl PartialEq for FeatureOverlays {
    fn eq(&self, other: &Self) -> bool {
        let covered = |a: &Self, b: &Self| {
            a.slots.iter().all(|(id, slot)| match b.slots.get(id) {
                Some(theirs) => slot.same_as(theirs.as_ref()),
                None => slot.is_default(),
            })
        };
        covered(self, other) && covered(other, self)
    }
}

#[cfg(test)]
#[path = "feature_overlays_tests.rs"]
mod tests;
