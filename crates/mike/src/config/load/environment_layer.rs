use figment::{Figment, providers::Serialized, value::Value};

use crate::environment::Environment;

const PREFIX: &str = "MIKE_";

pub(super) fn merge_prefixed(figment: Figment, environment: &dyn Environment) -> Figment {
    layer_entries(environment)
        .into_iter()
        .fold(figment, |figment, (key, value)| {
            figment.merge(Serialized::default(&key, value))
        })
}

pub(super) fn layer_entries(environment: &dyn Environment) -> Vec<(String, Value)> {
    let mut variables = environment.vars();
    variables.sort();
    variables
        .into_iter()
        .filter_map(|(name, raw)| {
            let key = field_key(&name)?;
            let value = raw.parse::<Value>().unwrap_or_else(|never| match never {});
            Some((key, value))
        })
        .collect()
}

pub(super) fn field_key(variable: &str) -> Option<String> {
    if variable.is_empty() {
        return None;
    }
    let trimmed = variable.trim();
    let head = trimmed.get(..PREFIX.len())?;
    if !head.eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let name = &trimmed[PREFIX.len()..];
    let renamed = match name.to_ascii_lowercase().as_str() {
        "listen_addr" => "proxy_listen_addr",
        "inspect_addr" => "inspect_listen_addr",
        _ => name,
    };
    let nested = renamed.replace("__", ".");
    let key = nested.trim();
    if key.split('.').any(str::is_empty) {
        return None;
    }
    Some(key.to_ascii_lowercase())
}

#[cfg(test)]
#[path = "environment_layer_tests.rs"]
mod environment_layer_tests;
