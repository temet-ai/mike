use serde_json::json;

use super::*;
use crate::tools::wrap_success;

fn arguments(value: Value) -> Arguments {
    value.as_object().cloned()
}

fn refusal(name: &str, value: Value) -> anyhow::Result<String> {
    check_arguments(name, arguments(value))
        .err()
        .map(|error| error.message.to_string())
        .ok_or_else(|| anyhow::anyhow!("expected {name} to refuse the arguments"))
}

#[test]
fn check_arguments_find_text_query_alias_is_renamed_and_reported() -> anyhow::Result<()> {
    let (checked, corrections) = check_arguments("find_text", arguments(json!({"query": "abc"})))?;
    assert_eq!(checked, arguments(json!({"needle": "abc"})));
    assert_eq!(
        corrections,
        vec![("query".to_string(), "needle".to_string())]
    );
    Ok(())
}

#[test]
fn with_corrections_adds_arguments_corrected_beside_the_data() {
    let result = wrap_success("find", json!({"matches": []}));
    let corrected = with_corrections(result, &vec![("query".to_string(), "needle".to_string())]);
    assert_eq!(
        corrected.structured_content,
        Some(json!({
            "find": {"matches": []},
            "arguments_corrected": [{"sent": "query", "used": "needle"}]
        }))
    );
}

#[test]
fn with_corrections_without_corrections_leaves_the_result_alone() {
    let result = wrap_success("find", json!({"matches": []}));
    let untouched = with_corrections(result.clone(), &Corrections::new());
    assert_eq!(untouched.structured_content, result.structured_content);
}

#[test]
fn check_arguments_alias_is_not_applied_when_the_real_argument_is_present() -> anyhow::Result<()> {
    let message = refusal("find_text", json!({"query": "abc", "needle": "real"}))?;
    assert!(
        message
            .starts_with("unknown argument `query` for `find_text`; it takes: needle (required)")
    );
    Ok(())
}

#[test]
fn check_arguments_unknown_name_lists_the_tool_arguments_with_required_marked() -> anyhow::Result<()>
{
    let message = refusal("read_project_knowledge", json!({"ids": ["a"]}))?;
    assert_eq!(
        message,
        "unknown argument `ids` for `read_project_knowledge`; it takes: reference (required), project, select"
    );
    Ok(())
}

#[test]
fn check_arguments_several_unknown_names_are_all_named() -> anyhow::Result<()> {
    let message = refusal("read_project_knowledge", json!({"a": 1, "b": 2}))?;
    assert!(message.starts_with("unknown arguments `a`, `b` for `read_project_knowledge`"));
    Ok(())
}

#[test]
fn check_arguments_select_is_always_allowed() -> anyhow::Result<()> {
    let sent = arguments(json!({"reference": "note:1", "select": ["/knowledge"]}));
    let (checked, corrections) = check_arguments("read_project_knowledge", sent.clone())?;
    assert_eq!(checked, sent);
    assert!(corrections.is_empty());
    Ok(())
}

#[test]
fn check_arguments_tool_without_a_closed_schema_passes_arguments_through() -> anyhow::Result<()> {
    let sent = arguments(json!({"anything": 1}));
    let (checked, corrections) = check_arguments("no_such_tool", sent.clone())?;
    assert_eq!(checked, sent);
    assert!(corrections.is_empty());
    Ok(())
}

#[test]
fn check_arguments_missing_arguments_pass_through_for_the_handler_to_refuse() -> anyhow::Result<()>
{
    let (checked, corrections) = check_arguments("find_text", None)?;
    assert_eq!(checked, None);
    assert!(corrections.is_empty());
    Ok(())
}

#[test]
fn alias_table_targets_are_declared_and_aliases_are_not() -> anyhow::Result<()> {
    for (tool, rules) in ALIASES {
        let declared = catalogue()
            .get(*tool)
            .ok_or_else(|| anyhow::anyhow!("{tool} has no closed schema"))?;
        for (alias, target) in *rules {
            assert!(declared.has(target), "{tool} does not declare {target}");
            assert!(!declared.has(alias), "{tool} declares the alias {alias}");
        }
    }
    Ok(())
}

#[test]
fn check_arguments_accepts_every_declared_name_of_every_closed_tool() -> anyhow::Result<()> {
    for (tool, declared) in catalogue() {
        let sent: Map<String, Value> = declared
            .names
            .iter()
            .map(|name| (name.clone(), Value::Null))
            .collect();
        let (checked, corrections) = check_arguments(tool, Some(sent.clone()))?;
        assert_eq!(checked, Some(sent), "{tool} changed its declared arguments");
        assert!(corrections.is_empty(), "{tool} corrected a declared name");
    }
    Ok(())
}

#[test]
fn check_arguments_session_id_passes_through_on_the_three_session_tagged_tools()
-> anyhow::Result<()> {
    let cases = [
        (
            "search_project_knowledge",
            json!({"project": "p", "session_id": "s"}),
        ),
        (
            "recall_similar_turns",
            json!({"project_path": "p", "query": "q", "session_id": "s"}),
        ),
        (
            "recall_project_context",
            json!({"project_path": "p", "session_id": "s"}),
        ),
    ];
    for (tool, sent) in cases {
        let (checked, corrections) = check_arguments(tool, arguments(sent.clone()))?;
        assert_eq!(checked, arguments(sent), "{tool} changed session_id");
        assert!(corrections.is_empty());
    }
    Ok(())
}

#[test]
fn check_arguments_session_id_is_unknown_on_a_tool_outside_the_three() -> anyhow::Result<()> {
    let message = refusal(
        "read_project_knowledge",
        json!({"reference": "x", "session_id": "s"}),
    )?;
    assert!(message.starts_with("unknown argument `session_id` for `read_project_knowledge`"));
    Ok(())
}
