use super::*;

fn scores(pairs: &[(usize, f32)]) -> Vec<RerankScore> {
    pairs
        .iter()
        .map(|&(index, score)| RerankScore { index, score })
        .collect()
}

#[test]
fn arrange_with_scores_puts_scored_items_first_in_score_order() {
    let arranged = arrange(vec!["a", "b", "c"], &scores(&[(2, 0.9), (0, 0.4)]));

    assert_eq!(
        arranged,
        vec![("c", Some(0.9)), ("a", Some(0.4)), ("b", None)]
    );
}

#[test]
fn arrange_without_scores_keeps_the_plain_order() {
    let arranged = arrange(vec![1, 2, 3], &[]);

    assert_eq!(arranged, vec![(1, None), (2, None), (3, None)]);
}

#[test]
fn arrange_with_a_score_for_a_missing_item_ignores_it() {
    let arranged = arrange(vec!["a", "b"], &scores(&[(7, 0.9), (1, 0.5)]));

    assert_eq!(arranged, vec![("b", Some(0.5)), ("a", None)]);
}

#[test]
fn arrange_with_a_repeated_index_uses_the_item_once() {
    let arranged = arrange(vec!["a", "b"], &scores(&[(1, 0.9), (1, 0.8)]));

    assert_eq!(arranged, vec![("b", Some(0.9)), ("a", None)]);
}

#[test]
fn report_from_loading_error_names_the_wait() {
    let report = RerankReport::from_error(&RerankError::Loading);

    assert_eq!(report.state, RerankState::Loading);
    assert_eq!(report.ready_in_s, Some(COLD_START_ESTIMATE_SECS));
    assert_eq!(
        report.detail.as_deref(),
        Some("the reranker is loading, ready in about 4 s; these results are in plain order")
    );
}

#[test]
fn report_from_timeout_error_is_unavailable_with_the_limit() {
    let report = RerankReport::from_error(&RerankError::Timeout(800));

    assert_eq!(report.state, RerankState::Unavailable);
    assert_eq!(report.ready_in_s, None);
    assert!(
        report
            .detail
            .is_some_and(|d| d.contains("800 ms") && d.ends_with("plain order"))
    );
}

#[test]
fn report_from_disabled_error_is_off() {
    let report = RerankReport::from_error(&RerankError::Disabled);

    assert_eq!(report.state, RerankState::Off);
    assert!(report.detail.is_some_and(|d| d.contains("rerank.enabled")));
}

#[test]
fn report_for_a_reranked_page_has_no_detail_and_is_ready() {
    let json = RerankReport::reranked().to_json();

    assert_eq!(json["state"], "reranked");
    assert_eq!(json["ready_in_s"], 0);
    assert!(json["detail"].is_null());
}

#[test]
fn report_from_refused_error_names_the_setting_to_change() {
    let report = RerankReport::from_error(&RerankError::Refused("http://omit:11435".into()));

    assert_eq!(report.state, RerankState::Unavailable);
    assert!(report.detail.is_some_and(
        |d| d.contains("http://omit:11435") && d.contains("semantic.allow_remote_embedding")
    ));
}
