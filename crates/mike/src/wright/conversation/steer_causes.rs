use serde_json::Value;

use super::Change;
use super::steer::{Fields, shown};

pub(crate) const GROUP_NAME_MAX_CHARS: usize = 80;
pub(crate) const CAUSE_IDS_MAX: usize = 200;
pub(crate) const PRIORITY_LIMIT: f64 = 1e12;

impl Fields<'_> {
    fn positive(&self, field: &str, what: &str) -> Result<i64, String> {
        let id = self.int(field)?;
        if id < 1 {
            return Err(format!("steer.{field} is {id}; {what} is 1 or more"));
        }
        Ok(id)
    }

    fn cause_ids(&self) -> Result<Vec<i64>, String> {
        let value = self.map.get("cause_ids");
        let Some(items) = value.and_then(Value::as_array) else {
            return Err(format!(
                "steer.cause_ids is {}; {} needs a list of cause ids there",
                shown(value),
                self.kind
            ));
        };
        let mut ids = Vec::with_capacity(items.len());
        for (at, item) in items.iter().enumerate() {
            match item.as_i64().filter(|id| *id >= 1) {
                Some(id) => ids.push(id),
                None => {
                    return Err(format!(
                        "steer.cause_ids[{at}] is {item}; a cause id is a whole number of 1 or more"
                    ));
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Err(format!(
                "steer.cause_ids is empty; {} needs at least one cause id",
                self.kind
            ));
        }
        if ids.len() > CAUSE_IDS_MAX {
            return Err(format!(
                "steer.cause_ids names {} causes; at most {CAUSE_IDS_MAX} at once",
                ids.len()
            ));
        }
        Ok(ids)
    }

    fn priority(&self) -> Result<Option<f64>, String> {
        match self.map.get("priority") {
            Some(Value::Null) => Ok(None),
            Some(value) => match value.as_f64() {
                Some(number) if number.is_finite() && number.abs() <= PRIORITY_LIMIT => {
                    Ok(Some(number))
                }
                _ => Err(format!(
                    "steer.priority is {value}; it must be null or a number between -{PRIORITY_LIMIT:e} and {PRIORITY_LIMIT:e}"
                )),
            },
            None => Err(format!(
                "steer.priority is missing; {} needs a number, or null to clear the order you set",
                self.kind
            )),
        }
    }

    fn merge(&self) -> Result<Change, String> {
        self.only(&["into", "cause_ids"])?;
        let into = self.positive("into", "a cause id")?;
        let cause_ids = self.cause_ids()?;
        if cause_ids.contains(&into) {
            return Err(format!(
                "steer.cause_ids contains {into}, the cause the others merge into"
            ));
        }
        Ok(Change::MergeCauses { into, cause_ids })
    }

    fn group(&self) -> Result<Change, String> {
        self.only(&["name", "cause_ids"])?;
        Ok(Change::GroupCauses {
            name: self.text("name", GROUP_NAME_MAX_CHARS)?,
            cause_ids: self.cause_ids()?,
        })
    }
}

pub(super) fn parse(fields: &Fields<'_>) -> Result<Change, String> {
    match fields.kind {
        "pin_cause" => {
            fields.only(&["cause_id"])?;
            Ok(Change::PinCause {
                cause_id: fields.positive("cause_id", "a cause id")?,
            })
        }
        "unpin_cause" => {
            fields.only(&["cause_id"])?;
            Ok(Change::UnpinCause {
                cause_id: fields.positive("cause_id", "a cause id")?,
            })
        }
        "set_cause_priority" => {
            fields.only(&["cause_id", "priority"])?;
            Ok(Change::SetCausePriority {
                cause_id: fields.positive("cause_id", "a cause id")?,
                priority: fields.priority()?,
            })
        }
        "merge_causes" => fields.merge(),
        "separate_merge" => {
            fields.only(&["merge_id"])?;
            Ok(Change::SeparateMerge {
                merge_id: fields.positive("merge_id", "a merge id")?,
            })
        }
        "group_causes" => fields.group(),
        _ => {
            fields.only(&["cause_ids"])?;
            Ok(Change::UngroupCauses {
                cause_ids: fields.cause_ids()?,
            })
        }
    }
}
