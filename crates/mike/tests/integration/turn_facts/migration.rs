use super::fixture;

#[tokio::test]
async fn migration_turn_facts_creates_table_and_toggle_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let table: Option<String> = sqlx::query_scalar("SELECT to_regclass('memory.turn_facts')::text")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(table.as_deref(), Some("memory.turn_facts"));
    let toggle: (bool, bool) = sqlx::query_as(
        "SELECT enabled, operator_set FROM config.feature_toggles WHERE key = 'dossier_builder'",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        toggle,
        (false, false),
        "dossier_builder ships off by default"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn feature_toggle_keys_are_closed_in_code_not_by_the_table() -> anyhow::Result<()> {
    // Migration 20261112160000 dropped feature_toggles_key_check: a new toggle needs
    // no migration. FeatureToggleService refuses unknown keys
    // (integration::feature_toggles_service::set_unknown_key_returns_not_found).
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let outcome = sqlx::query(
        "INSERT INTO config.feature_toggles (key, enabled, operator_set)
         VALUES ('not_a_toggle', true, false)",
    )
    .execute(f.handle.pool())
    .await;
    assert!(
        outcome.is_ok(),
        "the table no longer lists toggle keys: {outcome:?}"
    );
    f.db.cleanup().await?;
    Ok(())
}
