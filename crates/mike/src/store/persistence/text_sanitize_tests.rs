use serde_json::json;

use super::*;

#[test]
fn clean_text_without_nul_borrows_the_input() {
    assert!(matches!(clean_text("plain"), Cow::Borrowed("plain")));
}

#[test]
fn clean_text_with_nul_replaces_each_nul_with_the_stand_in() {
    assert_eq!(clean_text("a\0b\0"), "a\u{2400}b\u{2400}");
}

#[test]
fn clean_json_with_nul_in_nested_strings_and_keys_replaces_them_all() {
    let mut value = json!({"k\u{0}": ["x\u{0}y", {"inner": "\u{0}"}], "n": 3});
    assert!(clean_json(&mut value));
    assert_eq!(
        value,
        json!({"k\u{2400}": ["x\u{2400}y", {"inner": "\u{2400}"}], "n": 3})
    );
    let serialized = serde_json::to_string(&value).unwrap_or_default();
    assert!(!serialized.contains("\\u0000"), "{serialized}");
}

#[test]
fn clean_json_without_nul_reports_no_change() {
    let mut value = json!({"a": ["b", 1, null]});
    assert!(!clean_json(&mut value));
}

#[test]
fn body_carries_nul_finds_the_escape_and_the_raw_byte() {
    assert!(body_carries_nul(br#"{"t":"a\u0000b"}"#));
    assert!(body_carries_nul(b"a\0b"));
    assert!(!body_carries_nul(br#"{"t":"ab"}"#));
}

#[test]
fn request_with_clean_text_cleans_prompt_fields_only_when_needed() {
    let session = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(None, &session);
    assert!(matches!(request_with_clean_text(&req), Cow::Borrowed(_)));
    req.human_prompt = "say \0".to_string();
    req.last_user_message = "\0".to_string();
    let cleaned = request_with_clean_text(&req);
    assert_eq!(cleaned.human_prompt, "say \u{2400}");
    assert_eq!(cleaned.last_user_message, "\u{2400}");
}
