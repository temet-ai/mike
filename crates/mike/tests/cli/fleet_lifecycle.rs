use mike::launch::cli::test_support;

#[tokio::test]
async fn stopped_fleet_explicit_settings_cannot_bypass_actual_open_fence() -> anyhow::Result<()> {
    test_support::stopped_fleet_refuses_actual_open().await
}

#[tokio::test]
async fn accepted_open_reservation_holds_stop_until_backend_open_is_durable() -> anyhow::Result<()>
{
    test_support::accepted_open_blocks_stop_until_durable().await
}

#[tokio::test]
async fn backend_open_error_keeps_fleet_reservation_unresolved() -> anyhow::Result<()> {
    test_support::failed_backend_open_stays_unresolved().await
}

#[tokio::test]
async fn kill_unconfirmed_outcome_never_queues_a_closure() -> anyhow::Result<()> {
    test_support::unconfirmed_kill_does_not_record_closure().await
}

#[tokio::test]
async fn kill_confirmed_pane_preserves_live_or_unknown_root_evidence() -> anyhow::Result<()> {
    test_support::confirmed_pane_closure_preserves_exit_evidence().await
}

#[test]
fn root_origin_requires_exact_local_markers_and_surviving_witness() -> anyhow::Result<()> {
    test_support::root_origin_contract()
}
