use super::*;
use crate::store::views::MemoryBreakdownView;

fn assert_per_field_aggregates_match_seeded_sizes(view: &MemoryBreakdownView) {
    assert_eq!(view.entry_count, 3);
    assert_eq!(view.request_body_bytes, 5 + 100 + 1000);
    assert_eq!(view.response_body_bytes, 2 + 50 + 500);
    assert_eq!(view.filtered_request_body_bytes, 0);
    assert_eq!(view.tool_calls_bytes, 6);
    assert_eq!(view.tool_results_bytes, 6);
    assert_eq!(view.tool_results_in_body_bytes, 6);
    assert_eq!(view.system_prompt_blocks_bytes, 0);

    let expected_total: u64 = (5 + 100 + 1000) + (2 + 50 + 500) + 6 + 6 + 6;
    assert_eq!(view.entries_total_bytes, expected_total);
}

fn assert_other_in_memory_state_matches_session_counts(view: &MemoryBreakdownView) {
    assert_eq!(view.session_index_keys, 3);
    assert_eq!(view.composable_session_index_keys, 0);
    assert_eq!(view.session_meta_keys, 3);
}

fn assert_handler_filled_fields_stay_at_defaults(view: &MemoryBreakdownView) {
    assert_eq!(view.in_flight_count, 0);
    assert_eq!(view.process_rss_bytes, None);
}

fn assert_top_n_ranks_heaviest_first(view: &MemoryBreakdownView) {
    assert_eq!(view.top_entries.len(), 2);
    assert_eq!(view.top_entries[0].id, RequestId::from_raw(103));
    assert_eq!(view.top_entries[1].id, RequestId::from_raw(102));
    assert!(
        view.top_entries
            .iter()
            .all(|row| row.id != RequestId::from_raw(101))
    );
}

fn assert_heaviest_row_mirrors_source_request(view: &MemoryBreakdownView) {
    let heaviest = &view.top_entries[0];
    assert_eq!(heaviest.session_id, "sess-c");
    assert_eq!(heaviest.request_body_bytes, 1000);
    assert_eq!(heaviest.response_body_bytes, 500);
    assert_eq!(heaviest.filtered_request_body_bytes, 0);
    assert_eq!(heaviest.total_bytes, 1000 + 500 + 2 + 2 + 2);
}

fn assert_budget_context_reflects_disabled_budget(view: &MemoryBreakdownView) {
    assert_eq!(view.max_entries, 10);
    assert_eq!(view.max_store_bytes, 0);
    assert!(!view.over_byte_budget);
}

#[test]
fn memory_breakdown_aggregates_per_field_and_ranks_top_n() -> anyhow::Result<()> {
    use bytes::Bytes;

    let mut store = RequestStore::new(small_policy(10));

    let mut small = make_test_request(Some(RequestId::from_raw(101)), "sess-a");
    small.request_body = Some(Bytes::from_static(b"hello"));
    small.response_body = Some(Bytes::from_static(b"hi"));

    let mut medium = make_test_request(Some(RequestId::from_raw(102)), "sess-b");
    medium.request_body = Some(Bytes::from(vec![0u8; 100]));
    medium.response_body = Some(Bytes::from(vec![0u8; 50]));

    let mut large = make_test_request(Some(RequestId::from_raw(103)), "sess-c");
    large.request_body = Some(Bytes::from(vec![0u8; 1000]));
    large.response_body = Some(Bytes::from(vec![0u8; 500]));

    store.insert(small);
    store.insert(medium);
    store.insert(large);

    let view = store.memory_breakdown(2);

    assert_per_field_aggregates_match_seeded_sizes(&view);
    assert_other_in_memory_state_matches_session_counts(&view);
    assert_handler_filled_fields_stay_at_defaults(&view);
    assert_top_n_ranks_heaviest_first(&view);
    assert_heaviest_row_mirrors_source_request(&view);
    assert_budget_context_reflects_disabled_budget(&view);

    Ok(())
}
