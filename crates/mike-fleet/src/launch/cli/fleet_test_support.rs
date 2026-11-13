#[cfg(all(feature = "test-seams", not(test)))]
pub async fn stopped_fleet_refuses_actual_open() -> anyhow::Result<()> {
    super::open::admission_tests::stopped_fleet_explicit_settings_cannot_bypass_actual_open_fence()
        .await
}

#[cfg(all(feature = "test-seams", not(test)))]
pub async fn accepted_open_blocks_stop_until_durable() -> anyhow::Result<()> {
    super::open::admission_tests::accepted_open_reservation_holds_stop_until_backend_open_is_durable()
        .await
}

#[cfg(all(feature = "test-seams", not(test)))]
pub async fn failed_backend_open_stays_unresolved() -> anyhow::Result<()> {
    super::open::admission_tests::backend_open_error_keeps_fleet_reservation_unresolved().await
}
