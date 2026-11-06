use super::payload::payload_session_link;
use super::*;

#[test]
fn a_worker_post_turn_summary_rollup_comes_from_its_request() {
    let body = br#"{"worker_epoch":1,"external_metadata":{"post_turn_summary":
        {"status_category":"completed","status_detail":"ran the tests",
         "needs_action":"","recent_action":"ran cargo test"}}}"#;
    let signals = RemoteControlRoute {
        cse_id: "cse_01HRXsUQqSxtgFFa4Je2TGmP",
        suffix: "/worker",
    }
    .channel_signals(Some(body), None);
    assert_eq!(
        signals.worker_status, None,
        "this shape carries no worker_status"
    );
    assert_eq!(
        signals.post_turn_summary,
        Some(serde_json::json!({
            "status_category": "completed",
            "status_detail": "ran the tests",
            "needs_action": "",
            "recent_action": "ran cargo test",
        })),
        "external_metadata.post_turn_summary is kept exactly as the client sent it"
    );
}

#[test]
fn a_worker_status_call_carries_no_post_turn_summary() {
    let signals = RemoteControlRoute {
        cse_id: "cse_01HRXsUQqSxtgFFa4Je2TGmP",
        suffix: "/worker",
    }
    .channel_signals(Some(br#"{"worker_epoch":8,"worker_status":"idle"}"#), None);
    assert_eq!(signals.worker_status.as_deref(), Some("idle"));
    assert_eq!(
        signals.post_turn_summary, None,
        "the two PUT /worker shapes are mutually exclusive on live traffic"
    );
}

fn session_link_item(payload_type: &str, session_id: Option<&str>, cwd: Option<&str>) -> Vec<u8> {
    let session_id = session_id.map_or("null".to_string(), |s| format!(r#""{s}""#));
    let cwd = cwd.map_or("null".to_string(), |c| format!(r#""{c}""#));
    format!(
        r#"{{"payload":{{"type":"{payload_type}","sessionId":{session_id},
             "cwd":{cwd},"version":"2.1.263"}}}}"#
    )
    .into_bytes()
}

#[test]
fn payload_session_link_reads_an_attachment_item() {
    let body = session_link_item(
        "attachment",
        Some("11111111-1111-4111-8111-111111111111"),
        Some("/repo"),
    );
    let Some(link) = payload_session_link("attachment", &body) else {
        panic!("an attachment item carrying sessionId must yield a link");
    };
    assert_eq!(link.session_uuid, "11111111-1111-4111-8111-111111111111");
    assert_eq!(link.cwd.as_deref(), Some("/repo"));
    assert_eq!(link.client_version.as_deref(), Some("2.1.263"));
}

#[test]
fn payload_session_link_reads_a_system_item() {
    let body = session_link_item("system", Some("22222222-2222-4222-8222-222222222222"), None);
    let Some(link) = payload_session_link("system", &body) else {
        panic!("a system item carrying sessionId must yield a link");
    };
    assert_eq!(link.session_uuid, "22222222-2222-4222-8222-222222222222");
    assert_eq!(
        link.cwd, None,
        "system payloads carry the session id only sometimes, never a cwd guess"
    );
}

#[test]
fn payload_session_link_ignores_every_other_payload_type() {
    let body = session_link_item("result", Some("33333333-3333-4333-8333-333333333333"), None);
    assert_eq!(
        payload_session_link("result", &body),
        None,
        "only attachment and system items have been observed carrying sessionId"
    );
}

#[test]
fn payload_session_link_is_none_without_a_session_id() {
    let body = session_link_item("attachment", None, Some("/repo"));
    assert_eq!(
        payload_session_link("attachment", &body),
        None,
        "a cwd with no sessionId is not enough to link a channel"
    );
}

#[test]
fn payload_session_link_is_none_for_unparseable_bytes() {
    assert_eq!(payload_session_link("attachment", b"not json"), None);
    assert_eq!(payload_session_link("attachment", b"{}"), None);
}

#[test]
fn plan_request_surfaces_the_first_kept_items_session_link() {
    let Ok(attachment) = String::from_utf8(session_link_item(
        "attachment",
        Some("44444444-4444-4444-8444-444444444444"),
        Some("/work/repo"),
    )) else {
        panic!("session_link_item always produces valid utf8");
    };
    let body = format!(
        r#"{{"events":[{{"payload":{{"type":"stream_event","uuid":"u"}}}},{attachment}]}}"#
    )
    .into_bytes();
    let plan = RemoteControlRoute {
        cse_id: "cse_01HRXsUQqSxtgFFa4Je2TGmP",
        suffix: "/worker/events",
    }
    .plan_request(Some(&body));
    let Some(link) = plan.payload_link else {
        panic!("the kept attachment item's link must surface on the plan");
    };
    assert_eq!(link.session_uuid, "44444444-4444-4444-8444-444444444444");
    assert_eq!(link.cwd.as_deref(), Some("/work/repo"));
}

#[test]
fn plan_request_has_no_session_link_when_no_kept_item_carries_one() {
    let items = ["stream_event", "assistant"]
        .iter()
        .map(|t| format!(r#"{{"payload":{{"type":"{t}","uuid":"u"}},"ephemeral":true}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(r#"{{"events":[{items}]}}"#).into_bytes();
    let plan = RemoteControlRoute {
        cse_id: "cse_01HRXsUQqSxtgFFa4Je2TGmP",
        suffix: "/worker/events",
    }
    .plan_request(Some(&body));
    assert_eq!(
        plan.payload_link, None,
        "neither item is kept, so there is nothing to read a link from"
    );
}
