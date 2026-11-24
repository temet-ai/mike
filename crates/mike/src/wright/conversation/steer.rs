use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::steer_causes as causes;

pub(crate) const MAX_SPAN_DAYS: i64 = 7;
pub(crate) const NAME_MAX_CHARS: usize = 120;
pub(crate) const RESOURCE_MAX_CHARS: usize = 200;
pub(crate) const KINDS: [&str; 12] = [
    "pause_remedy",
    "resume_remedy",
    "open_problem",
    "set_engineer_limit",
    "skip_problem",
    "pin_cause",
    "unpin_cause",
    "set_cause_priority",
    "merge_causes",
    "separate_merge",
    "group_causes",
    "ungroup_causes",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LimitField {
    DailyStarts,
    WallSeconds,
    ToolCalls,
    Tokens,
}

impl LimitField {
    pub(crate) const ALL: [LimitField; 4] = [
        LimitField::DailyStarts,
        LimitField::WallSeconds,
        LimitField::ToolCalls,
        LimitField::Tokens,
    ];

    pub(crate) fn code(self) -> &'static str {
        match self {
            LimitField::DailyStarts => "daily_starts",
            LimitField::WallSeconds => "wall_seconds",
            LimitField::ToolCalls => "tool_calls",
            LimitField::Tokens => "tokens",
        }
    }

    pub(crate) fn range(self) -> (i64, i64) {
        match self {
            LimitField::DailyStarts => (0, 20),
            LimitField::WallSeconds => (60, 7_200),
            LimitField::ToolCalls => (1, 500),
            LimitField::Tokens => (10_000, 5_000_000),
        }
    }

    fn from_code(code: &str) -> Option<LimitField> {
        LimitField::ALL
            .into_iter()
            .find(|field| field.code() == code)
    }

    pub(crate) fn declared() -> String {
        LimitField::ALL
            .map(|field| {
                let (low, high) = field.range();
                format!("{} {low}..={high}", field.code())
            })
            .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Change {
    PauseRemedy {
        name: String,
        until: DateTime<Utc>,
    },
    ResumeRemedy {
        name: String,
    },
    OpenProblem {
        check: String,
        resource: String,
    },
    SetEngineerLimit {
        field: LimitField,
        value: i64,
    },
    SkipProblem {
        id: i64,
        until: DateTime<Utc>,
    },
    PinCause {
        cause_id: i64,
    },
    UnpinCause {
        cause_id: i64,
    },
    SetCausePriority {
        cause_id: i64,
        priority: Option<f64>,
    },
    MergeCauses {
        into: i64,
        cause_ids: Vec<i64>,
    },
    SeparateMerge {
        merge_id: i64,
    },
    GroupCauses {
        name: String,
        cause_ids: Vec<i64>,
    },
    UngroupCauses {
        cause_ids: Vec<i64>,
    },
}

impl Change {
    pub(crate) fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub(super) struct Fields<'a> {
    pub(super) kind: &'a str,
    pub(super) map: &'a Map<String, Value>,
}

pub(super) fn shown(value: Option<&Value>) -> String {
    value.map_or_else(|| "missing".to_owned(), Value::to_string)
}

fn fields(proposal: &Value) -> Result<Fields<'_>, String> {
    let Value::Object(map) = proposal else {
        return Err(format!(
            "steer is `{proposal}`; it must be an object whose kind is one of {}",
            KINDS.join(", ")
        ));
    };
    let Some(kind) = map.get("kind").and_then(Value::as_str) else {
        return Err(format!(
            "steer.kind is {}; it must be one of {}",
            shown(map.get("kind")),
            KINDS.join(", ")
        ));
    };
    Ok(Fields { kind, map })
}

impl Fields<'_> {
    pub(super) fn only(&self, allowed: &[&str]) -> Result<(), String> {
        let extra = self
            .map
            .keys()
            .find(|key| key.as_str() != "kind" && !allowed.contains(&key.as_str()));
        match extra {
            Some(extra) => Err(format!(
                "steer.{extra} is not a field of {}; its fields are {}",
                self.kind,
                allowed.join(", ")
            )),
            None => Ok(()),
        }
    }

    pub(super) fn text(&self, field: &str, max: usize) -> Result<String, String> {
        let value = self.map.get(field);
        match value.and_then(Value::as_str).map(str::trim) {
            Some(text) if !text.is_empty() && text.chars().count() <= max => Ok(text.to_owned()),
            _ => Err(format!(
                "steer.{field} is {}; {} needs text of 1 to {max} characters there",
                shown(value),
                self.kind
            )),
        }
    }

    pub(super) fn int(&self, field: &str) -> Result<i64, String> {
        let value = self.map.get(field);
        value.and_then(Value::as_i64).ok_or_else(|| {
            format!(
                "steer.{field} is {}; {} needs a whole number there",
                shown(value),
                self.kind
            )
        })
    }

    fn until(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
        let raw = self.text("until", 64)?;
        let until = DateTime::parse_from_rfc3339(&raw)
            .map_err(|e| format!("steer.until is `{raw}`; it must be an RFC 3339 time ({e})"))?
            .with_timezone(&Utc);
        let latest = now + Duration::days(MAX_SPAN_DAYS);
        if until <= now || until > latest {
            return Err(format!(
                "steer.until is `{raw}`; it must be after {} and no later than {}",
                now.to_rfc3339(),
                latest.to_rfc3339()
            ));
        }
        Ok(until)
    }

    fn limit(&self) -> Result<Change, String> {
        self.only(&["field", "value"])?;
        let code = self.text("field", 64)?;
        let field = LimitField::from_code(&code).ok_or_else(|| {
            format!(
                "steer.field is `{code}`; the engineer limits are {}",
                LimitField::declared()
            )
        })?;
        let value = self.int("value")?;
        let (low, high) = field.range();
        if !(low..=high).contains(&value) {
            return Err(format!(
                "steer.value is {value}; {code} must be between {low} and {high}"
            ));
        }
        Ok(Change::SetEngineerLimit { field, value })
    }

    fn skip(&self, now: DateTime<Utc>) -> Result<Change, String> {
        self.only(&["id", "until"])?;
        let id = self.int("id")?;
        if id < 1 {
            return Err(format!("steer.id is {id}; a problem id is 1 or more"));
        }
        Ok(Change::SkipProblem {
            id,
            until: self.until(now)?,
        })
    }
}

pub(crate) fn parse(proposal: &Value, now: DateTime<Utc>) -> Result<Change, String> {
    let fields = fields(proposal)?;
    match fields.kind {
        "pause_remedy" => {
            fields.only(&["name", "until"])?;
            Ok(Change::PauseRemedy {
                name: fields.text("name", NAME_MAX_CHARS)?,
                until: fields.until(now)?,
            })
        }
        "resume_remedy" => {
            fields.only(&["name"])?;
            Ok(Change::ResumeRemedy {
                name: fields.text("name", NAME_MAX_CHARS)?,
            })
        }
        "open_problem" => {
            fields.only(&["check", "resource"])?;
            Ok(Change::OpenProblem {
                check: fields.text("check", NAME_MAX_CHARS)?,
                resource: fields.text("resource", RESOURCE_MAX_CHARS)?,
            })
        }
        "set_engineer_limit" => fields.limit(),
        "skip_problem" => fields.skip(now),
        "pin_cause" | "unpin_cause" | "set_cause_priority" | "merge_causes" | "separate_merge"
        | "group_causes" | "ungroup_causes" => causes::parse(&fields),
        other => Err(format!(
            "steer.kind is `{other}`; Wright can only propose {}",
            KINDS.join(", ")
        )),
    }
}
