use super::maintenance_budget::{MaintenancePass, RefusalReason};
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistHandle, test_support};

#[tokio::test]
async fn maintenance_counter_is_atomic_apart_from_the_proposer_counter() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    assert!(!handle.lessons().reserve_lesson_maintenance_call(0).await?);
    let (first, second) = tokio::join!(
        handle.lessons().reserve_lesson_maintenance_call(1),
        handle.lessons().reserve_lesson_maintenance_call(1)
    );
    assert_ne!(first?, second?);
    assert!(!handle.lessons().reserve_lesson_maintenance_call(1).await?);
    assert!(handle.lessons().reserve_lesson_maintenance_call(2).await?);
    assert!(
        handle.lessons().reserve_lesson_proposer_call(1).await?,
        "maintenance calls must not use up the corrections allowance"
    );
    assert!(!handle.lessons().reserve_lesson_proposer_call(1).await?);
    assert!(
        !handle.lessons().reserve_lesson_maintenance_call(2).await?,
        "corrections calls must not be counted as maintenance"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn maintenance_reservation_fails_when_the_store_is_unreachable() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle.pool().close().await;
    assert!(
        handle
            .lessons()
            .reserve_lesson_maintenance_call(10)
            .await
            .is_err()
    );
    assert!(
        handle
            .lessons()
            .record_maintenance_refusal(MaintenancePass::Shape, RefusalReason::DailyBudget, 10)
            .await
            .is_err()
    );
    assert!(
        handle
            .lessons()
            .recent_maintenance_refusals()
            .await
            .is_err()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_refusal_is_logged_once_an_hour_per_pass_and_listed_while_recent() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let shape = |reason| {
        handle
            .lessons()
            .record_maintenance_refusal(MaintenancePass::Shape, reason, 400)
    };
    assert!(shape(RefusalReason::DailyBudget).await?);
    assert!(!shape(RefusalReason::DailyBudget).await?);
    assert!(
        handle
            .lessons()
            .record_maintenance_refusal(MaintenancePass::Placement, RefusalReason::DailyBudget, 400)
            .await?,
        "each pass has its own hourly allowance"
    );

    sqlx::query(
        "UPDATE memory.lesson_maintenance_refusals
            SET logged_at = now() - interval '61 minutes' WHERE pass = 'shape'",
    )
    .execute(handle.pool())
    .await?;
    assert!(shape(RefusalReason::ProviderLimit).await?);
    assert!(!shape(RefusalReason::ProviderLimit).await?);

    let seen = handle.lessons().recent_maintenance_refusals().await?;
    let summary: Vec<(&str, &str, i64)> = seen
        .iter()
        .map(|r| (r.pass.as_str(), r.reason.as_str(), r.cap))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("placement", "daily_budget", 400),
            ("shape", "provider_limit", 400)
        ]
    );

    sqlx::query(
        "UPDATE memory.lesson_maintenance_refusals
            SET refused_at = now() - interval '3 hours' WHERE pass = 'placement'",
    )
    .execute(handle.pool())
    .await?;
    let seen = handle.lessons().recent_maintenance_refusals().await?;
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].pass, "shape");
    db.cleanup().await?;
    Ok(())
}
