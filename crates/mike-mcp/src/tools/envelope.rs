use rmcp::model::{CallToolResult, ContentBlock, ErrorData as RmcpError};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::select::project_envelope;
use super::select_hint::available_roots;

pub(super) fn arguments_as_value(arguments: Option<Map<String, Value>>) -> Value {
    Value::Object(arguments.unwrap_or_default())
}

pub(super) fn parse_args_required<T>(arguments: Option<Map<String, Value>>) -> Result<T, RmcpError>
where
    T: for<'de> Deserialize<'de>,
{
    let map = arguments.ok_or_else(|| RmcpError::invalid_params("missing arguments", None))?;
    serde_json::from_value(Value::Object(map))
        .map_err(|e| RmcpError::invalid_params(format!("invalid arguments: {e}"), None))
}

pub(super) fn parse_args_default<T>(arguments: Option<Map<String, Value>>) -> Result<T, RmcpError>
where
    T: for<'de> Deserialize<'de> + Default,
{
    let value = match arguments {
        Some(map) => Value::Object(map),
        None => return Ok(T::default()),
    };
    serde_json::from_value(value)
        .map_err(|e| RmcpError::invalid_params(format!("invalid arguments: {e}"), None))
}

fn result_from_envelope(envelope: Value) -> CallToolResult {
    let mut result = CallToolResult::structured(envelope.clone());
    let pretty = serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| envelope.to_string());
    result.content = vec![ContentBlock::text(pretty)];
    result
}

pub(super) fn wrap_success_with_note(
    envelope_key: &str,
    inner: Value,
    note: Option<String>,
) -> CallToolResult {
    wrap_success_paged(envelope_key, inner, note, None)
}

pub(super) fn wrap_success_paged(
    envelope_key: &str,
    inner: Value,
    note: Option<String>,
    next_cursor: Option<String>,
) -> CallToolResult {
    let envelope = Value::Object({
        let mut m = Map::new();
        m.insert(envelope_key.to_string(), inner);
        if let Some(n) = note {
            m.insert("window_note".to_string(), Value::String(n));
        }
        if let Some(c) = next_cursor {
            m.insert("next_cursor".to_string(), Value::String(c));
        }
        m
    });
    result_from_envelope(envelope)
}

pub(super) fn wrap_success(envelope_key: &str, inner: Value) -> CallToolResult {
    let envelope = Value::Object({
        let mut m = Map::new();
        m.insert(envelope_key.to_string(), inner);
        m
    });
    result_from_envelope(envelope)
}

type Arguments = Option<Map<String, Value>>;

pub(super) fn extract_select(arguments: Arguments) -> Result<(Arguments, Vec<String>), RmcpError> {
    let mut arguments = arguments;
    let raw = arguments.as_mut().and_then(|m| m.remove("select"));
    let selectors = match raw {
        Some(value) => parse_select_value(&value)?,
        None => Vec::new(),
    };
    Ok((arguments, selectors))
}

fn parse_select_value(raw: &Value) -> Result<Vec<String>, RmcpError> {
    let elements = raw.as_array().ok_or_else(|| {
        RmcpError::invalid_params("`select` must be an array of JSON pointer strings", None)
    })?;
    elements
        .iter()
        .map(|element| {
            let selector = element.as_str().ok_or_else(|| {
                RmcpError::invalid_params(
                    format!(
                        "`select` elements must be strings (RFC 6901 JSON pointers), got {element}"
                    ),
                    None,
                )
            })?;
            if !selector.is_empty() && !selector.starts_with('/') {
                return Err(RmcpError::invalid_params(
                    format!(
                        "invalid `select` pointer {selector:?}: must be empty or start with \
                         '/' (RFC 6901)"
                    ),
                    None,
                ));
            }
            Ok(selector.to_string())
        })
        .collect()
}

pub(super) fn apply_projection(result: CallToolResult, selectors: &[String]) -> CallToolResult {
    if selectors.is_empty() || result.is_error == Some(true) {
        return result;
    }
    let Some(structured) = result.structured_content.as_ref() else {
        return result;
    };
    let (mut projected, unmatched) = project_envelope(structured, selectors);
    if let Value::Object(map) = &mut projected {
        map.insert(
            "select_applied".to_string(),
            Value::Array(selectors.iter().cloned().map(Value::String).collect()),
        );
        if !unmatched.is_empty() {
            map.insert(
                "available_roots".to_string(),
                available_roots(structured, &unmatched),
            );
            map.insert(
                "select_unmatched".to_string(),
                Value::Array(unmatched.into_iter().map(Value::String).collect()),
            );
        }
    }
    result_from_envelope(projected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, Default, PartialEq)]
    struct ExampleArgs {
        id: Option<u64>,
    }

    #[derive(Debug, Deserialize)]
    struct ExampleRequiredArgs {
        id: u64,
    }

    #[test]
    fn parse_args_default_missing_arguments_yields_default() -> anyhow::Result<()> {
        let args: ExampleArgs = parse_args_default(None)?;
        assert_eq!(args, ExampleArgs::default());
        Ok(())
    }

    #[test]
    fn parse_args_required_missing_arguments_yields_error() {
        let result = parse_args_required::<ExampleRequiredArgs>(None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_args_required_invalid_id_type_yields_invalid_params_error() {
        let mut m = Map::new();
        m.insert("id".to_string(), Value::String("not-a-u64".to_string()));
        let result = parse_args_required::<ExampleRequiredArgs>(Some(m));
        assert!(result.is_err());
    }

    #[test]
    fn parse_args_required_valid_id_yields_struct() -> anyhow::Result<()> {
        let mut m = Map::new();
        m.insert("id".to_string(), Value::Number(42.into()));
        let args: ExampleRequiredArgs = parse_args_required(Some(m))?;
        assert_eq!(args.id, 42);
        Ok(())
    }
}
