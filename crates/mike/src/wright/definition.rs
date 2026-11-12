use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::wright::watch::forks::ForkParams;
use crate::wright::watch::logs::LogParams;
use crate::wright::{Cadence, Probe, Source};

pub(crate) const NAME_MIN: usize = 3;
pub(crate) const NAME_MAX: usize = 63;

pub(crate) fn validate_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if (NAME_MIN..=NAME_MAX).contains(&name.len()) && starts_with_letter && rest_ok {
        return Ok(());
    }
    Err(format!(
        "name is `{name}`; it must be {NAME_MIN} to {NAME_MAX} characters of a-z, 0-9 and _, starting with a letter"
    ))
}

pub(crate) fn parse_source(code: &str) -> Result<Source, String> {
    Source::from_code(code).ok_or_else(|| {
        format!(
            "source is `{code}`; it must be one of {}",
            Source::ALL.map(Source::code).join(", ")
        )
    })
}

pub(crate) fn parse_cadence(code: &str) -> Result<Cadence, String> {
    Cadence::from_code(code).ok_or_else(|| {
        format!(
            "cadence is `{code}`; it must be one of {}",
            Cadence::ALL.map(Cadence::code).join(", ")
        )
    })
}

pub(crate) fn parse_probe(code: &str) -> Result<Probe, String> {
    Probe::from_code(code).ok_or_else(|| {
        format!(
            "probe is `{code}`; it must be one of {}",
            Probe::ALL.map(Probe::code).join(", ")
        )
    })
}

pub(crate) fn validate_expects(expects: &str) -> Result<(), String> {
    if expects.trim().is_empty() {
        return Err("expects is blank; it must say what a healthy run looks like".to_owned());
    }
    Ok(())
}

pub(crate) fn int_param(params: &Value, key: &str, min: i64, max: i64) -> Result<i64, String> {
    let value = params.get(key).ok_or_else(|| {
        format!(
            "params.{key} is missing; it must be an integer from {min} to {max} (params: {params})"
        )
    })?;
    let number = value.as_i64().ok_or_else(|| {
        format!("params.{key} is {value}; it must be an integer from {min} to {max}")
    })?;
    if (min..=max).contains(&number) {
        return Ok(number);
    }
    Err(format!(
        "params.{key} is {number}; it must be an integer from {min} to {max}"
    ))
}

pub(crate) fn validate_params(probe: Probe, params: &Value) -> Result<(), String> {
    if !params.is_object() {
        return Err(format!("params is {params}; it must be a JSON object"));
    }
    match probe {
        Probe::Sql => Ok(()),
        Probe::Logs => LogParams::from_value(params).map(drop),
        Probe::ColdForks => ForkParams::from_value(params).map(drop),
    }
}

static PARAM_READ: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\$1\s*->>?\s*'([A-Za-z0-9_]+)'").ok());

pub(crate) fn query_param_keys(query: &str) -> BTreeSet<String> {
    let Some(pattern) = PARAM_READ.as_ref() else {
        return BTreeSet::new();
    };
    pattern
        .captures_iter(query)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

pub(crate) fn validate_query_params(query: &str, params: &Value) -> Result<(), String> {
    let missing: Vec<String> = query_param_keys(query)
        .into_iter()
        .filter(|key| params.get(key).is_none_or(Value::is_null))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the query reads params {} but they are missing, so the check would never find anything (params: {params})",
        missing.join(", ")
    ))
}

pub(crate) fn merge_params(current: &Value, patch: &Value) -> Result<Value, String> {
    let Some(patch) = patch.as_object() else {
        return Err(format!("params is {patch}; it must be a JSON object"));
    };
    let mut merged: Map<String, Value> = current.as_object().cloned().unwrap_or_default();
    for (key, value) in patch {
        if value.is_null() {
            merged.remove(key);
        } else {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(merged))
}

pub(crate) fn validate_query(probe: Probe, query: Option<&str>) -> Result<(), String> {
    match (probe, query.map(str::trim)) {
        (Probe::Sql, Some(q)) if !q.is_empty() => Ok(()),
        (Probe::Sql, _) => Err("a sql probe needs a non-blank query".to_owned()),
        (_, None) => Ok(()),
        (other, Some(_)) => Err(format!(
            "query applies only to a sql probe; this check's probe is {}",
            other.code()
        )),
    }
}
