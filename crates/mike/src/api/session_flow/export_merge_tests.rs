use chrono::Utc;

use super::export::{merge_capture_complete, merge_coverage, merge_reason, merge_retention};
use crate::store::persistence::session_flow as persist;

fn base_coverage() -> persist::FlowCoverage {
    persist::FlowCoverage {
        component: persist::FlowComponent::ManifestStages,
        lower_id: None,
        upper_id: None,
        returned_through_id: None,
        exhausted: true,
        retention: persist::FlowRetention::Unknown,
        capture_complete: None,
        latest_only: false,
        best_effort: true,
        omitted_components: vec![],
        server_observed_at: Utc::now(),
        stale_reason: None,
    }
}

#[test]
fn merge_coverage_prefers_partial_retention_and_false_capture_complete() {
    let mut a = base_coverage();
    a.retention = persist::FlowRetention::Retained;
    a.capture_complete = Some(true);
    let mut b = base_coverage();
    b.retention = persist::FlowRetention::Partial;
    b.capture_complete = Some(false);
    b.stale_reason = Some("manifest_missing_malformed_or_unsupported: ...".into());
    let merged = merge_coverage(a, b);
    assert_eq!(merged.retention, persist::FlowRetention::Partial);
    assert_eq!(merged.capture_complete, Some(false));
    assert!(merged.stale_reason.is_some());
}

#[test]
fn merge_helpers_are_conservative_on_unknown_and_absent() {
    assert_eq!(
        merge_retention(
            persist::FlowRetention::Retained,
            persist::FlowRetention::Unknown
        ),
        persist::FlowRetention::Retained
    );
    assert_eq!(merge_capture_complete(Some(true), None), None);
    assert_eq!(merge_capture_complete(None, None), None);
    assert_eq!(merge_reason(None, Some("x".into())), Some("x".into()));
    assert_eq!(
        merge_reason(Some("x".into()), Some("x".into())),
        Some("x".into())
    );
    assert_eq!(
        merge_reason(Some("x".into()), Some("y".into())),
        Some("x; y".into())
    );
}
