use crate::feature_toggles::TOGGLE_META;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

#[tokio::test]
async fn every_toggle_key_in_code_is_accepted_by_the_feature_toggles_check() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut tx = handle.pool().begin().await?;
    let mut refused = Vec::new();
    for meta in TOGGLE_META {
        let saved = sqlx::query(
            "INSERT INTO config.feature_toggles (key, enabled, operator_set)
             VALUES ($1, false, false)
             ON CONFLICT (key) DO NOTHING",
        )
        .bind(meta.key)
        .execute(&mut *tx)
        .await;
        if saved.is_err() {
            refused.push(meta.key);
            tx.rollback().await?;
            tx = handle.pool().begin().await?;
        }
    }
    tx.rollback().await?;
    assert!(
        refused.is_empty(),
        "config.feature_toggles' key CHECK refuses {refused:?}: add a migration that widens \
         feature_toggles_key_check by these keys, or /settings answers 500 when they are set"
    );
    db.cleanup().await
}
