use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist as _;

use super::support::{drain_primary_turn, seed_session};

#[tokio::test]
async fn drain_never_injects_a_dead_lettered_mailbox_row_regardless_of_redelivery_toggle()
-> anyhow::Result<()> {
    for redelivery_enabled in [true, false] {
        let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await?
        else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&db.database_url)
            .await?;

        let to_uuid = seed_session(&pool, "proj-drain-drops-dead-lettered").await?;
        let msg = uuid::Uuid::new_v4();
        handle
            .launch()
            .insert_message(
                msg,
                to_uuid,
                None,
                "reaped before the recipient's next turn",
            )
            .await?;
        sqlx::query!(
            "UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1",
            msg,
        )
        .execute(&pool)
        .await?;

        let mut state =
            crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(handle.clone());
        state.feature_toggle_overlay.store(std::sync::Arc::new(
            crate::feature_toggles::FeatureToggles {
                mailbox_redelivery: redelivery_enabled,
                ..crate::feature_toggles::FeatureToggles::default()
            },
        ));
        let session_uuid_str = to_uuid.to_string();
        {
            let mut store = state.store.write().await;
            store.enqueue_reminder(
                &session_uuid_str,
                crate::store::models::SystemReminder {
                    source: crate::store::models::ReminderSource::SessionMailbox {
                        message_uuid: msg,
                        from_session_uuid: None,
                    },
                    payload: "<system-reminder>stale mailbox reminder</system-reminder>"
                        .to_string(),
                },
            );
        }

        let injected = drain_primary_turn(&state, &session_uuid_str).await;

        assert!(
            injected.is_empty(),
            "a dead-lettered mailbox row must never be injected \
                 (mailbox_redelivery={redelivery_enabled}): {injected:?}"
        );

        pool.close().await;
        db.cleanup().await?;
    }
    Ok(())
}
