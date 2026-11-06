use serde_json::json;

use crate::store::persistence::messages::canonicalize::write_canonical;
use crate::store::persistence::messages::delta::{
    COMPACTION_CONTINUATION_MARKER, DeltaRow, MARKER_LEADING_WINDOW_BYTES, classify_removed,
    is_reliable_sidecar_origin,
};
use crate::store::persistence::messages::{CanonicalMessage, compute_delta};

fn cm(ord: i32, sha: &str, content: &str) -> CanonicalMessage {
    let body = json!({"role": "user", "content": content});
    let mut canonical = Vec::new();
    if write_canonical(&mut canonical, &body).is_err() {
        canonical = serde_json::to_vec(&body).unwrap_or_default();
    }
    CanonicalMessage {
        sha: sha.to_string(),
        byte_size: canonical.len() as i32,
        body_canonical_bytes: canonical,
        role: "user".to_string(),
        ord,
        cache_control: None,
    }
}

fn bridge(n: i32) -> Vec<(i32, String)> {
    (0..n).map(|o| (o, format!("pred{o:0>4}"))).collect()
}

fn marker(text: &str) -> String {
    format!("{COMPACTION_CONTINUATION_MARKER}. {text}")
}

#[test]
fn classify_removed_compaction_when_collapsed_to_summary() {
    let pred = bridge(101);
    let removed: Vec<(i32, &str)> = pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
    let current = vec![cm(0, "summarysha", &marker("compacted summary"))];
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "compaction",
        "whole-prefix wipe + new head carrying the continuation marker = compaction"
    );
}

#[test]
fn classify_removed_short_turn_without_marker_is_not_compaction() {
    let pred = bridge(101);
    let removed: Vec<(i32, &str)> = pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
    let current = vec![cm(0, "freshturn", "an ordinary short turn, not a summary")];
    let reason = classify_removed(&pred, &removed, &current);
    assert_ne!(
        reason, "compaction",
        "a short turn with no continuation marker must NEVER be compaction"
    );
    assert_eq!(
        reason, "unknown",
        "whole-bridge wipe + no marker + no SHA overlap = unknown (D3)"
    );
}

#[test]
fn classify_removed_marker_deep_in_body_is_not_compaction() {
    let pred = bridge(101);
    let removed: Vec<(i32, &str)> = pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
    let filler: String = "filler-prose-with-no-marker-here. ".repeat(32);
    assert!(
        filler.len() >= 1024,
        "filler must be >= 1 KB so the marker lands past the leading window \
             (window is {MARKER_LEADING_WINDOW_BYTES} bytes)"
    );
    let deep_body = format!("{filler}{COMPACTION_CONTINUATION_MARKER}. trailing context");
    let current = vec![cm(0, "deepmarker", &deep_body)];
    let marker_offset = current[0]
        .body_canonical_bytes
        .windows(COMPACTION_CONTINUATION_MARKER.len())
        .position(|w| w == COMPACTION_CONTINUATION_MARKER.as_bytes());
    assert!(
        matches!(marker_offset, Some(o) if o > MARKER_LEADING_WINDOW_BYTES),
        "test setup invariant: marker must be present AND past the leading window \
             (offset={marker_offset:?}, window={MARKER_LEADING_WINDOW_BYTES})"
    );
    let reason = classify_removed(&pred, &removed, &current);
    assert_ne!(
        reason, "compaction",
        "a marker deep in the body must NEVER trigger compaction"
    );
    assert_eq!(
        reason, "unknown",
        "deep-body marker = not summary-shaped; D3 guard returns unknown"
    );
}

#[test]
fn classify_removed_compaction_when_marker_present_not_collapsed() {
    let pred = bridge(40);
    let removed: Vec<(i32, &str)> = pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
    let marker_body = marker("Analysis: prior work summarised");
    let mut current = vec![cm(0, "newhead", &marker_body)];
    for i in 1..5 {
        current.push(cm(i, &format!("new{i}"), "post-compaction turn"));
    }
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "compaction",
        "whole-prefix wipe + continuation marker = compaction even when not collapsed"
    );
}

#[test]
fn classify_removed_cross_lineage_whole_bridge_wipe_is_unknown() {
    let pred = bridge(80);
    let removed: Vec<(i32, &str)> = pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
    let current: Vec<CanonicalMessage> = (0..12)
        .map(|i| cm(i, &format!("other{i}"), "different-lineage conversation"))
        .collect();
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "unknown",
        "cross-lineage whole-bridge wipe must NOT masquerade as compaction"
    );
}

#[test]
fn classify_removed_tail_rebuild_unchanged() {
    let pred = bridge(158);
    let removed: Vec<(i32, &str)> = pred
        .iter()
        .filter(|(o, _)| *o >= 156)
        .map(|(o, s)| (*o, s.as_str()))
        .collect();
    let mut current: Vec<CanonicalMessage> = pred
        .iter()
        .filter(|(o, _)| *o < 156)
        .map(|(o, s)| cm(*o, s, "kept"))
        .collect();
    current.push(cm(156, "rebuilt", "rebuilt tail"));
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "tail_rebuild",
        "contiguous suffix removal with intact prefix stays tail_rebuild"
    );
}

#[test]
fn classify_removed_unknown_unchanged() {
    let pred = bridge(239);
    let removed: Vec<(i32, &str)> = pred
        .iter()
        .filter(|(o, _)| *o == 100 || *o == 150)
        .map(|(o, s)| (*o, s.as_str()))
        .collect();
    let current: Vec<CanonicalMessage> = pred
        .iter()
        .filter(|(o, _)| *o != 100 && *o != 150)
        .map(|(o, s)| cm(*o, s, "kept"))
        .collect();
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "unknown",
        "non-contiguous mid-conversation removal stays unknown"
    );
}

#[test]
fn classify_removed_prefix_wipe_without_summary_shape_is_not_compaction() {
    let pred = bridge(50);
    let removed: Vec<(i32, &str)> = pred
        .iter()
        .filter(|(o, _)| *o < 30)
        .map(|(o, s)| (*o, s.as_str()))
        .collect();
    let mut current = vec![cm(0, "freshhead", "a new turn, not a summary")];
    for (o, s) in pred.iter().filter(|(o, _)| *o >= 30) {
        current.push(cm(*o, s, "carried over"));
    }
    assert_eq!(
        classify_removed(&pred, &removed, &current),
        "unknown",
        "prefix wipe without collapse/marker corroboration is NOT compaction"
    );
}

#[test]
fn compute_delta_none_predecessor_is_all_additions() {
    let current = vec![cm(0, "a", "x"), cm(1, "b", "y")];
    let rows = compute_delta(&current, None);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.change_kind == "added"));
    assert!(rows.iter().all(|r| r.removed_reason.is_none()));
}

#[test]
fn compute_delta_compaction_emits_removed_rows_with_reason() {
    let pred = bridge(101);
    let current = vec![cm(0, "summarysha", &marker("the synthesised summary"))];
    let rows = compute_delta(&current, Some(pred.as_slice()));

    let added: Vec<&DeltaRow> = rows.iter().filter(|r| r.change_kind == "added").collect();
    let removed: Vec<&DeltaRow> = rows.iter().filter(|r| r.change_kind == "removed").collect();
    assert_eq!(added.len(), 1, "the summary message is the sole addition");
    assert_eq!(removed.len(), 101, "all 101 predecessor msgs removed");
    assert!(
        removed
            .iter()
            .all(|r| r.removed_reason == Some("compaction")),
        "every removed row carries removed_reason=compaction"
    );
}

#[test]
fn compute_delta_cross_lineage_wipe_never_compaction() {
    let pred = bridge(80);
    let current: Vec<CanonicalMessage> = (0..12)
        .map(|i| cm(i, &format!("other{i}"), "different lineage"))
        .collect();
    let rows = compute_delta(&current, Some(pred.as_slice()));
    let removed: Vec<&DeltaRow> = rows.iter().filter(|r| r.change_kind == "removed").collect();
    assert_eq!(removed.len(), 80);
    assert!(
        removed.iter().all(|r| r.removed_reason == Some("unknown")),
        "cross-lineage wipe classified unknown, not compaction"
    );
}

#[test]
fn is_reliable_sidecar_origin_excludes_subagent_and_primary() {
    assert!(is_reliable_sidecar_origin("title_sidecar"));
    assert!(is_reliable_sidecar_origin("advisor_sidecar"));
    assert!(is_reliable_sidecar_origin("quota_probe"));
    assert!(
        !is_reliable_sidecar_origin("subagent"),
        "subagent is the ambiguous mislabel — must NOT short-circuit"
    );
    assert!(!is_reliable_sidecar_origin("primary"));
    assert!(!is_reliable_sidecar_origin("primary_first"));
}

proptest::proptest! {
    #[test]
    fn prop_cross_lineage_wipe_never_compaction(
        pred_len in 3i32..200,
        cur_len in 3usize..40,
    ) {
        let pred = bridge(pred_len);
        let removed: Vec<(i32, &str)> =
            pred.iter().map(|(o, s)| (*o, s.as_str())).collect();
        let current: Vec<CanonicalMessage> = (0..cur_len)
            .map(|i| cm(i as i32, &format!("cur{i:0>5}"), "ordinary turn content"))
            .collect();
        let reason = classify_removed(&pred, &removed, &current);
        proptest::prop_assert_ne!(
            reason, "compaction",
            "whole-bridge wipe + no overlap + >2 non-marker msgs must not be compaction"
        );
    }
}
