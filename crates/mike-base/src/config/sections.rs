use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub trait ConfigSection:
    Serialize + DeserializeOwned + Default + Clone + Send + Sync + 'static
{
    const KEY: &'static str;
}

trait ErasedSection: Send + Sync {
    fn to_json(&self) -> serde_json::Value;
    fn as_any(&self) -> &dyn Any;
}

impl<T: ConfigSection> ErasedSection for T {
    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub struct SectionSlot(Arc<dyn ErasedSection>);

impl SectionSlot {
    fn of<T: ConfigSection>(value: T) -> Self {
        Self(Arc::new(value))
    }
}

pub struct SectionSpec {
    pub key: &'static str,
    default: fn() -> SectionSlot,
    parse: fn(&figment::value::Value) -> Result<SectionSlot, String>,
}

fn default_slot<T: ConfigSection>() -> SectionSlot {
    SectionSlot::of(T::default())
}

fn parse_slot<T: ConfigSection>(value: &figment::value::Value) -> Result<SectionSlot, String> {
    value
        .deserialize::<T>()
        .map(SectionSlot::of)
        .map_err(|error| error.to_string())
}

impl SectionSpec {
    pub const fn of<T: ConfigSection>() -> Self {
        Self {
            key: T::KEY,
            default: default_slot::<T>,
            parse: parse_slot::<T>,
        }
    }
}

#[derive(Clone)]
pub struct ConfigSections {
    slots: BTreeMap<&'static str, SectionSlot>,
}

impl ConfigSections {
    pub fn empty() -> Self {
        Self {
            slots: BTreeMap::new(),
        }
    }

    pub fn get<T: ConfigSection>(&self) -> T {
        self.slots
            .get(T::KEY)
            .and_then(|slot| slot.0.as_any().downcast_ref::<T>())
            .cloned()
            .unwrap_or_default()
    }

    pub fn set<T: ConfigSection>(&mut self, value: T) {
        self.slots.insert(T::KEY, SectionSlot::of(value));
    }

    pub fn update<T: ConfigSection>(&mut self, change: impl FnOnce(&mut T)) {
        let mut value = self.get::<T>();
        change(&mut value);
        self.set(value);
    }

    pub fn keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.slots.keys().copied()
    }
}

impl Default for ConfigSections {
    fn default() -> Self {
        let slots = crate::settings::registry::section_specs()
            .map(|spec| (spec.key, (spec.default)()))
            .collect();
        Self { slots }
    }
}

impl Serialize for ConfigSections {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            self.slots
                .iter()
                .map(|(key, slot)| (*key, slot.0.to_json())),
        )
    }
}

impl<'de> Deserialize<'de> for ConfigSections {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = BTreeMap::<String, figment::value::Value>::deserialize(deserializer)?;
        let mut slots = BTreeMap::new();
        for spec in crate::settings::registry::section_specs() {
            let slot = match raw.get(spec.key) {
                Some(value) => (spec.parse)(value).map_err(serde::de::Error::custom)?,
                None => (spec.default)(),
            };
            slots.insert(spec.key, slot);
        }
        Ok(Self { slots })
    }
}

#[cfg(test)]
#[path = "sections_tests.rs"]
mod tests;
