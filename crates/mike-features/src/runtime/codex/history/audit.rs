use std::collections::{BTreeMap, BTreeSet};

use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

use super::{HistoryError, HistoryErrorKind};

#[derive(Default)]
pub struct HistoryAudit {
    pub control_error: Option<HistoryError>,
    pub entry_errors: BTreeMap<usize, HistoryError>,
}

pub fn audit(bytes: &[u8], items: bool) -> Result<HistoryAudit, HistoryError> {
    let mut audit = HistoryAudit::default();
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    AuditSeed {
        scope: Scope::Frame,
        items,
        audit: &mut audit,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end())
    .map_err(|_| HistoryError::new(HistoryErrorKind::InvalidEnvelope, ""))?;
    Ok(audit)
}

#[derive(Clone, Copy)]
enum Scope {
    Frame,
    Result,
    Thread,
    Data,
    Turn(usize),
    Entry(usize),
    Item(usize),
}

impl Scope {
    fn watches(self, key: &str) -> bool {
        match self {
            Self::Frame => matches!(key, "id" | "method" | "result" | "error"),
            Self::Result => matches!(
                key,
                "data" | "nextCursor" | "backwardsCursor" | "thread" | "threadId" | "turnId"
            ),
            Self::Thread => matches!(key, "id" | "sessionId" | "historyMode" | "ephemeral"),
            Self::Turn(_) => matches!(key, "id" | "itemsView" | "threadId"),
            Self::Entry(_) => matches!(key, "turnId" | "threadId" | "item"),
            Self::Item(_) => matches!(key, "id" | "type"),
            Self::Data => false,
        }
    }

    fn child(self, key: &str) -> Option<Self> {
        match (self, key) {
            (Self::Frame, "result") => Some(Self::Result),
            (Self::Result, "thread") => Some(Self::Thread),
            (Self::Result, "data") => Some(Self::Data),
            (Self::Entry(index), "item") => Some(Self::Item(index)),
            _ => None,
        }
    }

    fn pointer(self) -> String {
        match self {
            Self::Frame => String::new(),
            Self::Result => "/result".into(),
            Self::Thread => "/result/thread".into(),
            Self::Data => "/result/data".into(),
            Self::Turn(index) | Self::Entry(index) => format!("/result/data/{index}"),
            Self::Item(index) => format!("/result/data/{index}/item"),
        }
    }

    fn record_duplicate(self, key: &str, audit: &mut HistoryAudit) {
        let error = HistoryError::new(
            HistoryErrorKind::DuplicateControlField,
            format!("{}/{key}", self.pointer()),
        );
        match self {
            Self::Turn(index) | Self::Entry(index) | Self::Item(index) => {
                audit.entry_errors.entry(index).or_insert(error);
            }
            _ => {
                audit.control_error.get_or_insert(error);
            }
        }
    }
}

struct AuditSeed<'a> {
    scope: Scope,
    items: bool,
    audit: &'a mut HistoryAudit,
}

impl<'de> DeserializeSeed<'de> for AuditSeed<'_> {
    type Value = ();

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AuditSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON history response")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if self.scope.watches(&key) && !seen.insert(key.clone()) {
                self.scope.record_duplicate(&key, self.audit);
            }
            if let Some(scope) = self.scope.child(&key) {
                map.next_value_seed(AuditSeed {
                    scope,
                    items: self.items,
                    audit: self.audit,
                })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        if !matches!(self.scope, Scope::Data) {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            return Ok(());
        }
        let mut index = 0;
        loop {
            let scope = if self.items {
                Scope::Entry(index)
            } else {
                Scope::Turn(index)
            };
            let value = seq.next_element_seed(AuditSeed {
                scope,
                items: self.items,
                audit: self.audit,
            })?;
            if value.is_none() {
                return Ok(());
            }
            index += 1;
        }
    }

    fn visit_bool<E: serde::de::Error>(self, _value: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: serde::de::Error>(self, _value: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: serde::de::Error>(self, _value: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E: serde::de::Error>(self, _value: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E: serde::de::Error>(self, _value: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
}
