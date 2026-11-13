use chrono::Utc;

use super::super::{
    PATH_DELIVERY, PATH_HEARTBEAT, PATH_STREAM, RemoteControlCall, RemoteControlSignals,
    RollupTimes,
};

fn call_at<'a>(cse_id: &'a str, method: &'a str, path: &'a str) -> RemoteControlCall<'a> {
    RemoteControlCall {
        cse_id,
        method,
        path,
        http_status: Some(200),
        request_body: None,
        request_body_sha256: None,
        request_body_size: None,
        response_body: None,
        response_body_sha256: None,
        response_body_size: None,
        created_at: Utc::now(),
        completed_at: Some(Utc::now()),
        signals: RemoteControlSignals::default(),
        payloads: &[],
    }
}

#[test]
fn each_path_advances_only_its_own_dated_column() {
    let heartbeat = RollupTimes::of(&call_at("cse_a", "POST", PATH_HEARTBEAT));
    assert!(heartbeat.heartbeat_at.is_some());
    assert!(heartbeat.delivery_at.is_none());
    assert!(heartbeat.stream_opened_at.is_none());

    let delivery = RollupTimes::of(&call_at("cse_a", "POST", PATH_DELIVERY));
    assert!(delivery.delivery_at.is_some());
    assert!(delivery.heartbeat_at.is_none());

    let stream = RollupTimes::of(&call_at("cse_a", "GET", PATH_STREAM));
    assert!(stream.stream_opened_at.is_some());
    assert!(stream.heartbeat_at.is_none());

    let push = RollupTimes::of(&call_at("cse_a", "POST", "/worker/events"));
    assert!(push.heartbeat_at.is_none());
    assert!(push.worker_status_at.is_none());
    assert!(push.delivery_at.is_none());
    assert!(push.stream_opened_at.is_none());
}

#[test]
fn a_status_is_dated_by_the_signal_not_the_path() {
    let read = RollupTimes::of(&call_at("cse_a", "GET", "/worker"));
    assert!(read.worker_status_at.is_none());

    let write = RollupTimes::of(&RemoteControlCall {
        signals: RemoteControlSignals {
            worker_status: Some("requires_action"),
            ..RemoteControlSignals::default()
        },
        ..call_at("cse_a", "PUT", "/worker")
    });
    assert!(write.worker_status_at.is_some());
}
