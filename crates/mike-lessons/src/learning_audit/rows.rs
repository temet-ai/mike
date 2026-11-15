use serde_json::Value;

pub(super) fn rows<'a>(data: &'a Value, name: &str) -> &'a [Value] {
    data.get(name)
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
}

pub(super) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(fields)) => !fields.is_empty(),
    }
}

pub(super) fn int(row: &Value, key: &str) -> i64 {
    match row.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float as i64))
            .unwrap_or(0),
        Some(Value::String(text)) => text.parse().unwrap_or(0),
        _ => 0,
    }
}

pub(super) fn float(row: &Value, key: &str) -> f64 {
    match row.get(key) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub(super) fn show(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

pub(super) fn text(row: &Value, key: &str) -> String {
    show(row.get(key))
}

pub(super) fn non_empty<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

pub(super) fn percent(share: f64) -> String {
    format!("{:.1}%", share * 100.0)
}

pub(super) fn rate_text(part: i64, whole: i64) -> String {
    if whole == 0 {
        return "no judged turn".to_owned();
    }
    let share = (part * 100) as f64 / whole as f64;
    format!("{part} of {whole} ({:.0}%)", share.round_ties_even())
}
