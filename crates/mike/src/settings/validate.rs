use super::{SETTINGS_META, SettingKind, SettingMeta};

fn validate_bool(key: &str, value: serde_json::Value) -> Result<serde_json::Value, String> {
    if value.is_boolean() {
        Ok(value)
    } else {
        Err(format!("setting '{key}' requires a boolean value"))
    }
}

fn validate_int(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let n = value
        .as_i64()
        .ok_or_else(|| format!("setting '{key}' requires an integer value"))?;
    if let Some(min) = meta.min_i64
        && n < min
    {
        return Err(format!("setting '{key}' must be >= {min}, got {n}"));
    }
    if let Some(max) = meta.max_i64
        && n > max
    {
        return Err(format!("setting '{key}' must be <= {max}, got {n}"));
    }
    Ok(serde_json::Value::Number(n.into()))
}

fn validate_float(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let f = value
        .as_f64()
        .ok_or_else(|| format!("setting '{key}' requires a float value"))?;
    if let Some(min) = meta.min_f64
        && f < min
    {
        return Err(format!("setting '{key}' must be >= {min}, got {f}"));
    }
    if let Some(max) = meta.max_f64
        && f > max
    {
        return Err(format!("setting '{key}' must be <= {max}, got {f}"));
    }
    Ok(serde_json::Value::Number(
        serde_json::Number::from_f64(f)
            .ok_or_else(|| format!("setting '{key}' value is NaN or infinite"))?,
    ))
}

fn validate_string(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let s = value
        .as_str()
        .ok_or_else(|| format!("setting '{key}' requires a string value"))?;
    if let Some(max) = meta.max_len
        && s.len() > max
    {
        return Err(format!(
            "setting '{key}' value exceeds max length {max} (got {})",
            s.len()
        ));
    }
    Ok(serde_json::Value::String(s.to_owned()))
}

fn validate_text(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let s = value
        .as_str()
        .ok_or_else(|| format!("setting '{key}' requires a string value"))?;
    if s.trim().is_empty() {
        return Err(format!(
            "setting '{key}' cannot be blank; reset it to the default instead"
        ));
    }
    if let Some(max) = meta.max_len
        && s.len() > max
    {
        return Err(format!(
            "setting '{key}' value exceeds max length {max} (got {})",
            s.len()
        ));
    }
    Ok(serde_json::Value::String(s.to_owned()))
}

fn validate_string_list(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let arr = value
        .as_array()
        .ok_or_else(|| format!("setting '{key}' requires an array of strings"))?;
    for (i, item) in arr.iter().enumerate() {
        let s = item
            .as_str()
            .ok_or_else(|| format!("setting '{key}' item [{i}] is not a string"))?;
        if let Some(max) = meta.max_len
            && s.len() > max
        {
            return Err(format!(
                "setting '{key}' item [{i}] exceeds max length {max} (got {})",
                s.len()
            ));
        }
    }
    Ok(value)
}

fn validate_enum(
    key: &str,
    value: serde_json::Value,
    meta: &SettingMeta,
) -> Result<serde_json::Value, String> {
    let s = value
        .as_str()
        .ok_or_else(|| format!("setting '{key}' requires a string enum value"))?;
    let variants = meta
        .enum_variants
        .ok_or_else(|| format!("setting '{key}' has no enum variants defined"))?;
    if !variants.contains(&s) {
        return Err(format!(
            "setting '{key}' value '{}' is not one of: {}",
            s,
            variants.join(", ")
        ));
    }
    Ok(serde_json::Value::String(s.to_owned()))
}

pub(crate) fn validate_setting_value(
    key: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let meta = SETTINGS_META
        .iter()
        .find(|m| m.key == key)
        .ok_or_else(|| format!("unknown setting key: {key}"))?;

    let validated = match meta.kind {
        SettingKind::Bool => validate_bool(key, value),
        SettingKind::Int => validate_int(key, value, meta),
        SettingKind::Float => validate_float(key, value, meta),
        SettingKind::String => validate_string(key, value, meta),
        SettingKind::Text => validate_text(key, value, meta),
        SettingKind::StringList => validate_string_list(key, value, meta),
        SettingKind::Enum => validate_enum(key, value, meta),
    }?;
    crate::llm::provider::validate_setting(key, &validated)?;
    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_setting_value_bool_ok() {
        let result =
            validate_setting_value("reminders.inject_lessons", serde_json::Value::Bool(false));
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn validate_setting_value_bool_wrong_type() {
        let result = validate_setting_value(
            "reminders.inject_lessons",
            serde_json::Value::Number(1u64.into()),
        );
        assert!(result.is_err(), "expected Err for wrong type");
    }

    #[test]
    fn validate_setting_value_compression_dedup_enabled_ok() {
        let result =
            validate_setting_value("compression_dedup.enabled", serde_json::Value::Bool(false));
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn validate_setting_value_int_range() {
        assert!(
            validate_setting_value(
                "reminders.max_lessons_to_inject",
                serde_json::Value::Number(5u64.into())
            )
            .is_ok()
        );
        assert!(
            validate_setting_value(
                "reminders.max_lessons_to_inject",
                serde_json::Value::Number(serde_json::Number::from(-1i64))
            )
            .is_err()
        );
        assert!(
            validate_setting_value(
                "reminders.max_lessons_to_inject",
                serde_json::Value::Number(serde_json::Number::from(51i64))
            )
            .is_err()
        );
    }

    #[test]
    fn validate_setting_value_float_range() {
        assert!(
            validate_setting_value(
                "steerer.refocus.confidence_threshold",
                serde_json::json!(0.75)
            )
            .is_ok()
        );
        assert!(
            validate_setting_value(
                "steerer.refocus.confidence_threshold",
                serde_json::json!(1.5)
            )
            .is_err()
        );
        assert!(
            validate_setting_value(
                "steerer.refocus.confidence_threshold",
                serde_json::json!(-0.1)
            )
            .is_err()
        );
    }

    #[test]
    fn validate_setting_value_enum_variants() {
        assert!(validate_setting_value("log_level", serde_json::json!("info")).is_ok());
        assert!(validate_setting_value("log_level", serde_json::json!("verbose")).is_err());
    }

    #[test]
    fn validate_setting_value_unknown_key() {
        let result = validate_setting_value("no_such_key", serde_json::Value::Bool(true));
        assert!(result.is_err());
    }

    #[test]
    fn validate_setting_value_text_blank_rejected() {
        let blank =
            validate_setting_value("weasel_intercept.correction_prompt", serde_json::json!(""));
        assert!(blank.is_err(), "empty string must be rejected");

        let whitespace_only = validate_setting_value(
            "weasel_intercept.correction_prompt",
            serde_json::json!("   \n\t "),
        );
        assert!(
            whitespace_only.is_err(),
            "whitespace-only string must be rejected"
        );

        let blank_rubric =
            validate_setting_value("judge_worker.system_prompt", serde_json::json!(""));
        assert!(blank_rubric.is_err(), "empty rubric must be rejected");

        let whitespace_rubric =
            validate_setting_value("judge_worker.system_prompt", serde_json::json!("   "));
        assert!(
            whitespace_rubric.is_err(),
            "whitespace-only rubric must be rejected"
        );
    }

    #[test]
    fn validate_setting_value_text_non_blank_accepted_unchanged() {
        let result = validate_setting_value(
            "weasel_intercept.correction_prompt",
            serde_json::json!("Say '{word}' is not allowed."),
        );
        assert_eq!(
            result,
            Ok(serde_json::Value::String(
                "Say '{word}' is not allowed.".to_owned()
            ))
        );
    }

    #[test]
    fn validate_setting_value_string_kind_allows_blank() {
        let result = validate_setting_value("judge_worker.model", serde_json::json!(""));
        assert_eq!(result, Ok(serde_json::Value::String(String::new())));
    }

    #[test]
    fn validate_setting_value_file_read_compression_min_bytes_range() {
        assert!(
            validate_setting_value("file_read_compression.min_bytes", serde_json::json!(1500))
                .is_ok()
        );
        assert!(
            validate_setting_value("file_read_compression.min_bytes", serde_json::json!(0))
                .is_err(),
            "0 is below the min_i64 floor"
        );
        assert!(
            validate_setting_value(
                "file_read_compression.min_bytes",
                serde_json::json!(20_000_000i64)
            )
            .is_err(),
            "20MB exceeds the max_i64 ceiling"
        );
    }

    #[test]
    fn validate_setting_value_file_read_compression_line_shape() {
        assert!(
            validate_setting_value("file_read_compression.head_lines", serde_json::json!(40))
                .is_ok()
        );
        assert!(
            validate_setting_value("file_read_compression.tail_lines", serde_json::json!(20))
                .is_ok()
        );
        assert!(
            validate_setting_value("file_read_compression.head_lines", serde_json::json!(-1))
                .is_err()
        );
    }

    #[test]
    fn validate_setting_value_file_read_compression_string_lists() {
        assert!(
            validate_setting_value(
                "file_read_compression.enabled_producers",
                serde_json::json!(["Read", "Bash"])
            )
            .is_ok()
        );
        assert!(
            validate_setting_value(
                "file_read_compression.protected_paths",
                serde_json::json!(["**/*.env", "secrets/**"])
            )
            .is_ok()
        );
        assert!(
            validate_setting_value(
                "file_read_compression.enabled_producers",
                serde_json::json!("Read")
            )
            .is_err(),
            "a bare string is not a StringList"
        );
    }
}
