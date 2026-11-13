use super::payload::{DUPLICATED_PAYLOAD_TYPES, RequestPlan};
use super::policy::{BodyCapture, KNOWN_ENDPOINTS};
use super::stored_body::StoredBody;
use super::*;

const REAL_ID: &str = "cse_01HRXsUQqSxtgFFa4Je2TGmP";

fn route(suffix: &str) -> RemoteControlRoute<'_> {
    RemoteControlRoute {
        cse_id: REAL_ID,
        suffix,
    }
}

#[test]
fn recognises_every_known_endpoint() {
    for endpoint in KNOWN_ENDPOINTS {
        let path = format!("/v1/code/sessions/{REAL_ID}{}", endpoint.suffix);
        let route = recognise(&path).unwrap_or_else(|| panic!("{path} must be recognised"));
        assert_eq!(route.cse_id, REAL_ID);
        assert_eq!(route.suffix, endpoint.suffix);
    }
}

#[test]
fn recognises_a_channel_path_with_no_suffix() {
    let path = format!("/v1/code/sessions/{REAL_ID}");
    let Some(route) = recognise(&path) else {
        panic!("a bare channel path is still a channel path");
    };
    assert_eq!(route.cse_id, REAL_ID);
    assert_eq!(route.suffix, "");
}

#[test]
fn recognises_an_unknown_suffix() {
    let path = format!("/v1/code/sessions/{REAL_ID}/worker/something-new");
    let Some(route) = recognise(&path) else {
        panic!("an unseen suffix must still be captured");
    };
    assert_eq!(route.suffix, "/worker/something-new");
}

#[test]
fn rejects_a_segment_without_the_cse_prefix() {
    assert_eq!(
        recognise("/v1/code/sessions/01HRXsUQqSxtgFFa4Je2TGmP/worker/heartbeat"),
        None
    );
    assert_eq!(recognise("/v1/code/sessions/abc/worker"), None);
}

#[test]
fn rejects_a_prefix_with_nothing_after_it() {
    assert_eq!(recognise("/v1/code/sessions/cse_"), None);
    assert_eq!(recognise("/v1/code/sessions/cse_/worker"), None);
}

#[test]
fn rejects_an_over_long_id() {
    let long_id = format!("cse_{}", "a".repeat(MAX_CSE_ID_LEN));
    assert!(long_id.len() > MAX_CSE_ID_LEN);
    assert_eq!(
        recognise(&format!("/v1/code/sessions/{long_id}/worker")),
        None
    );
}

#[test]
fn rejects_an_over_long_suffix() {
    let long_suffix = format!("/{}", "a".repeat(MAX_SUFFIX_LEN));
    assert!(long_suffix.len() > MAX_SUFFIX_LEN);
    assert_eq!(
        recognise(&format!("/v1/code/sessions/{REAL_ID}{long_suffix}")),
        None
    );
}

#[test]
fn rejects_the_bare_sessions_path() {
    assert_eq!(recognise("/v1/code/sessions"), None);
    assert_eq!(recognise("/v1/code/sessions/"), None);
}

#[test]
fn rejects_inference_paths() {
    assert_eq!(recognise("/v1/messages"), None);
    assert_eq!(recognise("/v1/messages/count_tokens"), None);
    assert_eq!(recognise("/v1/code/sessions_other/cse_abc/worker"), None);
}

#[test]
fn rejects_an_id_carrying_path_traversal() {
    assert_eq!(recognise("/v1/code/sessions/cse_../../etc/passwd"), None);
    assert_eq!(recognise("/v1/code/sessions/cse_a%2Fb/worker"), None);
}

#[test]
fn endpoint_labels_are_bounded() {
    assert_eq!(
        route("/worker/heartbeat").endpoint_label(),
        "/worker/heartbeat"
    );
    assert_eq!(
        route("/worker/anything-a-caller-invented").endpoint_label(),
        "other",
        "an unknown suffix must not become its own metric series"
    );
    assert_eq!(route("").endpoint_label(), "other");
    assert_eq!(
        endpoint_labels().count(),
        KNOWN_ENDPOINTS.len() + 1,
        "the seeded label set must cover every value endpoint_label can return"
    );
    assert!(endpoint_labels().any(|label| label == "other"));
}

fn batch(types: &[&str]) -> Vec<u8> {
    let items: Vec<String> = types
        .iter()
        .map(|t| format!(r#"{{"payload":{{"type":"{t}","uuid":"u"}},"ephemeral":true}}"#))
        .collect();
    format!(r#"{{"events":[{}]}}"#, items.join(",")).into_bytes()
}

fn kept(plan: &RequestPlan<'_>) -> Vec<(String, bool)> {
    plan.payloads
        .iter()
        .map(|p| (p.payload_type.to_string(), p.body.is_some()))
        .collect()
}

#[test]
fn a_batch_keeps_what_we_capture_nowhere_else_and_elides_the_rest() {
    let body = batch(&[
        "stream_event",
        "permission_request",
        "assistant",
        "task_status",
    ]);
    let plan = route("/worker/events").plan_request(Some(&body));
    assert_eq!(
        kept(&plan),
        vec![
            ("stream_event".to_string(), false),
            ("permission_request".to_string(), true),
            ("assistant".to_string(), false),
            ("task_status".to_string(), true),
        ],
        "a permission prompt in a batch of transcript frames must survive it"
    );
    assert_eq!(
        plan.capture,
        BodyCapture::DigestOnly,
        "the payloads carry the bytes, so the batch body is measured rather than stored again"
    );
    assert_eq!(
        plan.payloads.iter().map(|p| p.ordinal).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "a kept item keeps its position in the batch it came from"
    );
}

#[test]
fn every_payload_is_measured_whether_or_not_its_bytes_are_kept() {
    let body = batch(&["stream_event", "permission_request"]);
    let plan = route("/worker/events").plan_request(Some(&body));
    let elided = &plan.payloads[0];
    let stored = &plan.payloads[1];
    assert!(elided.body.is_none());
    assert!(
        elided.body_size > 0,
        "an elided item still records its size"
    );
    assert_ne!(
        elided.body_sha256, stored.body_sha256,
        "each item is digested on its own bytes, so an elided one can still be matched"
    );
    assert_eq!(
        stored.body.map(<[u8]>::len),
        Some(stored.body_size as usize),
        "a stored item's bytes and its recorded size must agree"
    );
}

#[test]
fn every_duplicated_type_is_elided_and_nothing_else_is() {
    let events = route("/worker/events");
    for duplicated in DUPLICATED_PAYLOAD_TYPES {
        let body = batch(&[duplicated]);
        assert_eq!(
            kept(&events.plan_request(Some(&body))),
            vec![((*duplicated).to_string(), false)],
            "{duplicated} is already captured through /v1/messages"
        );
    }
    for novel in [
        "permission_request",
        "permission_response",
        "permission_denied",
        "permission_retry",
        "task_status",
        "session_state_changed",
        "session_context",
        "session_memory",
        "todo_reminder",
        "transcript_mirror",
    ] {
        let body = batch(&[novel]);
        assert_eq!(
            kept(&events.plan_request(Some(&body))),
            vec![(novel.to_string(), true)],
            "{novel} is captured nowhere else at all"
        );
    }
}

#[test]
fn an_unrecognised_payload_type_is_kept_rather_than_dropped() {
    let body = batch(&["a_type_from_a_later_client"]);
    assert_eq!(
        kept(&route("/worker/events").plan_request(Some(&body))),
        vec![("a_type_from_a_later_client".to_string(), true)],
        "the payload vocabulary is one client version's and will grow; defaulting to \
         discard would silently lose the next type"
    );
}

#[test]
fn a_single_payload_body_is_classified_the_same_way() {
    let internal = route("/worker/internal-events");
    let compaction = br#"{"payload":{"type":"session_state_changed","uuid":"u"},
         "is_compaction":true,"preserved_event_ids":["a","b"],
         "session_agent_id":"agent-1"}"#;
    let plan = internal.plan_request(Some(compaction));
    assert_eq!(
        kept(&plan),
        vec![("session_state_changed".to_string(), true)],
        "the compaction record and the subagent that produced it are captured nowhere else"
    );
    assert_eq!(
        plan.payloads[0].body,
        Some(&compaction[..]),
        "the stored item is the whole body, so is_compaction and preserved_event_ids \
         ride along with it"
    );

    let duplicate = br#"{"payload":{"type":"assistant","uuid":"u"},"is_compaction":false}"#;
    assert_eq!(
        kept(&internal.plan_request(Some(duplicate))),
        vec![("assistant".to_string(), false)]
    );
}

#[test]
fn a_body_that_is_not_a_batch_we_recognise_is_kept_whole() {
    let events = route("/worker/events");
    for body in [
        &b""[..],
        b"not json",
        b"{}",
        b"[]",
        br#"{"events":[]}"#,
        br#"{"events":[{"no_payload_here":1}]}"#,
        br#"{"events":"not an array"}"#,
    ] {
        let plan = events.plan_request(Some(body));
        assert_eq!(
            plan.capture,
            BodyCapture::Full,
            "an envelope we cannot read must cost storage, never data: {:?}",
            String::from_utf8_lossy(body)
        );
        assert!(
            plan.payloads.is_empty(),
            "a body we could not read reports no payloads: {:?}",
            String::from_utf8_lossy(body)
        );
    }
    let none = events.plan_request(None);
    assert_eq!(none.capture, BodyCapture::Full);
    assert!(none.payloads.is_empty());
}

#[test]
fn a_sibling_array_is_not_mistaken_for_the_batch() {
    let body = br#"{"tags":["a","b"],"events":[{"payload":{"type":"assistant"}}]}"#;
    assert_eq!(
        kept(&route("/worker/events").plan_request(Some(body))),
        vec![("assistant".to_string(), false)],
        "the batch is the array whose every element is a payload item, not the first array"
    );
}

#[test]
fn an_endpoint_with_no_payload_envelope_never_consults_one() {
    let heartbeat = route("/worker/heartbeat").plan_request(Some(br#"{"worker_epoch":7}"#));
    assert_eq!(heartbeat.capture, BodyCapture::Full);
    assert!(heartbeat.payloads.is_empty());

    let delivery_batch = batch(&["assistant"]);
    let delivery = route("/worker/events/delivery").plan_request(Some(&delivery_batch));
    assert_eq!(
        delivery.capture,
        BodyCapture::Full,
        "the delivery acknowledgement is not a transcript batch, whatever it happens to hold"
    );
    assert!(delivery.payloads.is_empty());
}

#[test]
fn the_credential_bearing_responses_are_never_stored() {
    for suffix in ["/bridge", "/worker/register"] {
        assert_eq!(
            route(suffix).response_capture(),
            BodyCapture::DigestOnly,
            "{suffix} answers with a worker_jwt, which must not reach the database"
        );
        assert_eq!(
            route(suffix).plan_request(None).capture,
            BodyCapture::Full,
            "{suffix} carries no credential on the way out"
        );
    }
}

#[test]
fn an_unknown_endpoint_stores_both_bodies_and_contributes_no_rollup() {
    let unknown = route("/worker/something-new");
    assert_eq!(unknown.plan_request(None).capture, BodyCapture::Full);
    assert_eq!(unknown.response_capture(), BodyCapture::Full);
    assert_eq!(
        unknown.channel_signals(
            Some(br#"{"worker_epoch":7}"#),
            Some(br#"{"worker_epoch":9}"#)
        ),
        ChannelSignals::default(),
        "a payload shape we have never seen must not be read as if we knew it"
    );
}

#[test]
fn a_heartbeat_rollup_comes_from_its_request() {
    let signals = route("/worker/heartbeat").channel_signals(
        Some(
            br#"{"session_id":"cse_01HRXsUQqSxtgFFa4Je2TGmP","worker_epoch":7,
                 "supports_heartbeat_probe":true,"current_interval_seconds":20,
                 "idle_seconds":143}"#,
        ),
        Some(br#"{"worker_epoch":999}"#),
    );
    assert_eq!(
        signals,
        ChannelSignals {
            client_session_id: Some("cse_01HRXsUQqSxtgFFa4Je2TGmP".to_string()),
            worker_epoch: Some(7),
            idle_seconds: Some(143),
            current_interval_seconds: Some(20),
            ..ChannelSignals::default()
        },
        "the response must not be read for an endpoint whose signals are in its request"
    );
}

#[test]
fn a_worker_status_rollup_comes_from_its_request() {
    let signals = route("/worker").channel_signals(
        Some(br#"{"worker_epoch":8,"worker_status":"requires_action"}"#),
        None,
    );
    assert_eq!(signals.worker_status.as_deref(), Some("requires_action"));
    assert_eq!(signals.worker_epoch, Some(8));
}

#[test]
fn a_bridge_rollup_comes_from_its_response() {
    let signals = route("/bridge").channel_signals(
        Some(b"{}"),
        Some(br#"{"worker_jwt":"a.live.token","worker_epoch":11,"expires_in":3600}"#),
    );
    assert_eq!(
        signals.worker_epoch,
        Some(11),
        "the epoch is the value worth keeping from a response whose bytes are dropped"
    );
}

#[test]
fn the_transcript_firehose_is_never_parsed_for_a_rollup() {
    assert_eq!(
        route("/worker/events").channel_signals(
            Some(br#"{"worker_epoch":7}"#),
            Some(br#"{"worker_epoch":7}"#)
        ),
        ChannelSignals::default(),
        "the highest-volume path must not pay for a JSON parse it cannot contribute to"
    );
}

#[test]
fn unparseable_and_unrelated_bodies_yield_nothing() {
    let heartbeat = route("/worker/heartbeat");
    let nothing = ChannelSignals::default();
    for body in [
        &b""[..],
        b"not json",
        b"[1,2,3]",
        br#"{"unrelated":"value"}"#,
        br#"{"worker_epoch":"seven"}"#,
    ] {
        assert_eq!(
            heartbeat.channel_signals(Some(body), None),
            nothing,
            "body {:?} must contribute nothing",
            String::from_utf8_lossy(body)
        );
    }
}

#[test]
fn a_call_with_no_body_is_told_apart_from_one_with_an_empty_body() {
    assert_eq!(
        stored_body(None, BodyCapture::Full),
        StoredBody::default(),
        "a call that carried no body records a NULL size"
    );
    let empty = stored_body(Some(b""), BodyCapture::Full);
    assert_eq!(empty.bytes, Some(&b""[..]));
    assert_eq!(
        empty.size,
        Some(0),
        "an empty body is still a body, recorded as size 0"
    );
    assert!(empty.sha256.is_some());
}

#[test]
fn an_elided_body_keeps_its_size_and_digest() {
    let payload = br#"{"events":[{"text":"a transcript chunk"}]}"#;
    let full = stored_body(Some(payload), BodyCapture::Full);
    let elided = stored_body(Some(payload), BodyCapture::DigestOnly);
    assert_eq!(full.bytes, Some(&payload[..]));
    assert_eq!(full.size, Some(payload.len() as i32));
    assert_eq!(elided.bytes, None);
    assert_eq!(
        (elided.sha256, elided.size),
        (full.sha256, full.size),
        "eliding the bytes must not change what is measured about them"
    );
}
