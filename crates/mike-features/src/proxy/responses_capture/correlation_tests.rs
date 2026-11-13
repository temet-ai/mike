use axum::http::{HeaderMap, HeaderValue};

use super::{TURN_METADATA_HEADER, correlate};

const SESSION: &str = "01a1cbf6-a0c6-7a13-84fb-fba08fd85ea7";

fn headers(pairs: &[(&'static str, &str)]) -> anyhow::Result<HeaderMap> {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(*name, HeaderValue::from_str(value)?);
    }
    Ok(map)
}

#[test]
fn correlate_turn_request_reads_session_thread_turn_and_workspace() -> anyhow::Result<()> {
    let metadata = format!(
        "{{\"session_id\":\"{SESSION}\",\"thread_id\":\"{SESSION}\",\"turn_id\":\"t-1\",\"request_kind\":\"turn\",\"workspaces\":{{\"/work/repo\":{{\"has_changes\":false}}}}}}"
    );
    let map = headers(&[
        ("session-id", SESSION),
        ("thread-id", SESSION),
        (TURN_METADATA_HEADER, &metadata),
    ])?;
    let found = correlate(&map, b"{}");
    assert_eq!(found.session_uuid, Some(uuid::Uuid::parse_str(SESSION)?));
    assert_eq!(found.provider_thread_id.as_deref(), Some(SESSION));
    assert_eq!(found.provider_turn_id.as_deref(), Some("t-1"));
    assert_eq!(found.request_kind.as_deref(), Some("turn"));
    assert_eq!(found.cwd.as_deref(), Some("/work/repo"));
    assert_eq!(found.evidence["session_id"]["from"], "header:session-id");
    Ok(())
}

#[test]
fn correlate_prewarm_with_empty_turn_id_reports_turn_unknown() -> anyhow::Result<()> {
    let metadata =
        format!("{{\"session_id\":\"{SESSION}\",\"turn_id\":\"\",\"request_kind\":\"prewarm\"}}");
    let map = headers(&[(TURN_METADATA_HEADER, &metadata)])?;
    let found = correlate(&map, b"");
    assert_eq!(found.provider_turn_id, None);
    assert!(found.evidence["turn_id"].get("unknown").is_some());
    assert_eq!(
        found.evidence["session_id"]["from"],
        format!("header:{TURN_METADATA_HEADER}")
    );
    Ok(())
}

#[test]
fn correlate_without_headers_takes_cwd_from_environment_context() {
    let body = br#"{"input":[{"content":[{"text":"<environment_context>\n  <cwd>/srv/app</cwd>\n</environment_context>"}]}]}"#;
    let found = correlate(&HeaderMap::new(), body);
    assert_eq!(found.cwd.as_deref(), Some("/srv/app"));
    assert_eq!(found.session_uuid, None);
    assert!(found.evidence["session_id"].get("unknown").is_some());
}

#[test]
fn correlate_non_uuid_session_id_links_no_session() -> anyhow::Result<()> {
    let map = headers(&[("session-id", "not-a-uuid")])?;
    let found = correlate(&map, b"");
    assert_eq!(found.provider_session_id.as_deref(), Some("not-a-uuid"));
    assert_eq!(found.session_uuid, None);
    assert!(found.evidence["session_uuid"].get("unknown").is_some());
    Ok(())
}
