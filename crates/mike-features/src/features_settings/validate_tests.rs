use serde_json::json;

use crate::settings::validate_setting_value;

fn validate(key: &str, value: serde_json::Value) -> Result<serde_json::Value, String> {
    super::register();
    validate_setting_value(key, value)
}

#[test]
fn validate_setting_value_bool_ok() {
    let result = validate("reminders.inject_lessons", json!(false));
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn validate_setting_value_bool_wrong_type() {
    let result = validate("reminders.inject_lessons", json!(1));
    assert!(
        result
            .as_ref()
            .is_err_and(|e| !e.starts_with("unknown setting key")),
        "expected a type error, got {result:?}"
    );
}

#[test]
fn validate_setting_value_compression_dedup_enabled_ok() {
    let result = validate("compression_dedup.enabled", json!(false));
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn validate_setting_value_int_range() {
    assert!(validate("reminders.max_lessons_to_inject", json!(5)).is_ok());
    assert!(validate("reminders.max_lessons_to_inject", json!(-1)).is_err());
    assert!(validate("reminders.max_lessons_to_inject", json!(51)).is_err());
}

#[test]
fn validate_setting_value_text_blank_rejected() {
    let blank = validate("weasel_intercept.correction_prompt", json!(""));
    assert!(
        blank
            .as_ref()
            .is_err_and(|e| !e.starts_with("unknown setting key")),
        "empty string must be rejected as blank, got {blank:?}"
    );
    let whitespace_only = validate("weasel_intercept.correction_prompt", json!("   \n\t "));
    assert!(
        whitespace_only.is_err(),
        "whitespace-only string must be rejected"
    );
}

#[test]
fn validate_setting_value_text_non_blank_accepted_unchanged() {
    let result = validate(
        "weasel_intercept.correction_prompt",
        json!("Say '{word}' is not allowed."),
    );
    assert_eq!(result, Ok(json!("Say '{word}' is not allowed.")));
}

#[test]
fn validate_setting_value_file_read_compression_min_bytes_range() {
    assert!(validate("file_read_compression.min_bytes", json!(1500)).is_ok());
    assert!(
        validate("file_read_compression.min_bytes", json!(0)).is_err(),
        "0 is below the min_i64 floor"
    );
    assert!(
        validate("file_read_compression.min_bytes", json!(20_000_000i64)).is_err(),
        "20MB exceeds the max_i64 ceiling"
    );
}

#[test]
fn validate_setting_value_file_read_compression_line_shape() {
    assert!(validate("file_read_compression.head_lines", json!(40)).is_ok());
    assert!(validate("file_read_compression.tail_lines", json!(20)).is_ok());
    assert!(validate("file_read_compression.head_lines", json!(-1)).is_err());
}

#[test]
fn validate_setting_value_file_read_compression_string_lists() {
    assert!(
        validate(
            "file_read_compression.enabled_producers",
            json!(["Read", "Bash"])
        )
        .is_ok()
    );
    assert!(
        validate(
            "file_read_compression.protected_paths",
            json!(["**/*.env", "secrets/**"])
        )
        .is_ok()
    );
    assert!(
        validate("file_read_compression.enabled_producers", json!("Read")).is_err(),
        "a bare string is not a StringList"
    );
}
