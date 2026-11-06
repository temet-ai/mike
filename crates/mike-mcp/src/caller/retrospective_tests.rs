use super::*;
use serde_json::json;

const SESSION: &str = "abcdef00-0000-4000-8000-000000000001";
const OTHER: &str = "00000000-0000-4000-8000-000000000002";

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn restricted_writes_bind_omitted_or_matching_identity() -> anyhow::Result<()> {
    for tool in ["write_session_note", "end_workspace_registration"] {
        for input in [json!({}), json!({"session_id": SESSION.to_uppercase()})] {
            let output = authorize(Profile::Retrospective, tool, args(input), Some(SESSION))?
                .ok_or_else(|| anyhow::anyhow!("missing arguments"))?;
            assert_eq!(output["session_id"], SESSION);
        }
    }
    Ok(())
}

#[test]
fn restricted_writes_reject_explicit_identity_mismatch_or_missing_context() {
    for tool in ["write_session_note", "end_workspace_registration"] {
        for identity in [json!(OTHER), Value::Null, json!(""), json!(42)] {
            let result = authorize(
                Profile::Retrospective,
                tool,
                args(json!({"session_id": identity})),
                Some(SESSION),
            );
            assert!(result.is_err(), "{tool} accepted {identity}");
        }
        for caller in [None, Some(""), Some("untrusted"), Some(OTHER)] {
            let result = authorize(
                Profile::Retrospective,
                tool,
                args(json!({"session_id": SESSION, "body": SESSION})),
                caller,
            );
            assert!(result.is_err(), "{tool} accepted caller {caller:?}");
        }
    }
}

#[test]
fn restricted_profile_denies_every_unlisted_tool() {
    for tool in crate::tools::all_tools() {
        let name = tool.name.as_ref();
        let result = authorize(Profile::Retrospective, name, None, Some(SESSION));
        assert_eq!(
            result.is_ok(),
            RETROSPECTIVE_TOOLS.contains(&name),
            "{name}"
        );
    }
    assert!(authorize(Profile::Retrospective, "unknown", None, Some(SESSION)).is_err());
}

#[test]
fn restricted_inspection_preserves_source_session_and_projection() -> anyhow::Result<()> {
    for name in ["list_session_notes", "get_session_overview"] {
        let input = args(json!({"session_id": OTHER, "select": ["/notes"]}));
        assert_eq!(
            authorize(Profile::Retrospective, name, input.clone(), None)?,
            input
        );
    }
    Ok(())
}

#[test]
fn findings_reject_updates_kinds_and_side_effect_payloads() {
    for input in [
        json!({"note_uuid": "existing"}),
        json!({"kind": "report"}),
        json!({"kind": "brief"}),
        json!({"kind": null}),
        json!({"data": {"carry": true}}),
        json!({"data": {"ruling": {"command": "make check"}}}),
        json!({"tags": ["carry:lesson"]}),
        json!({"tags": ["acceptance-obligation"]}),
    ] {
        let result = authorize(
            Profile::Retrospective,
            "write_session_note",
            args(input.clone()),
            Some(SESSION),
        );
        assert!(result.is_err(), "accepted {input}");
    }
}

#[test]
fn findings_keep_full_content_tags_data_and_select() -> anyhow::Result<()> {
    let input = json!({
        "title": "Finding", "body": "All of the evidence", "kind": "note",
        "tags": ["retrospective", format!("fleet:{OTHER}")],
        "data": {"evidence": ["source request"]}, "select": ["/note/note_uuid"]
    });
    let mut expected = input.clone();
    expected["session_id"] = json!(SESSION);
    assert_eq!(
        authorize(
            Profile::Retrospective,
            "write_session_note",
            args(input),
            Some(SESSION),
        )?,
        args(expected)
    );
    Ok(())
}

#[test]
fn ordinary_profiles_preserve_explicit_session_ids_and_unlisted_mutations() -> anyhow::Result<()> {
    for profile in [
        Profile::Core,
        Profile::Full,
        Profile::Worker,
        Profile::Coordinator,
        Profile::Acceptance,
        Profile::Observer,
        Profile::Reference,
    ] {
        for name in [
            "write_session_note",
            "end_workspace_registration",
            "define_fleet",
        ] {
            let input = args(json!({"session_id": OTHER, "note_uuid": "existing"}));
            assert_eq!(authorize(profile, name, input.clone(), None)?, input);
        }
    }
    Ok(())
}
