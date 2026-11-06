use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use mike::dossier::worker::{DossierWorkerDeps, run_dossier_builder};
use mike::feature_toggles::FeatureToggles;
use mike::store::models::{ProxyEvent, RequestId};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::{
    NewTurn, Req, enable_builder, fixture, insert_request_in, insert_session, insert_turn_in,
};

const SESSIONS: usize = 10;

#[tokio::test]
async fn lagged_broadcast_is_recovered_by_backstop() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .upsert_setting("dossier.backstop_interval_secs", &serde_json::json!(10))
        .await?;
    enable_builder(&f).await?;
    let mut turns = Vec::new();
    for index in 0..SESSIONS {
        let session = insert_session(f.handle.pool(), f.project_id, chrono::Utc::now()).await?;
        let request = 9100 + i64::try_from(index)?;
        insert_request_in(&f, session, &Req::primary(request, 0, "ask something")).await?;
        let turn = insert_turn_in(&f, session, &NewTurn::closed(request, request, 0, 50)).await?;
        sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2")
            .bind(turn)
            .bind(request)
            .execute(f.handle.pool())
            .await?;
        turns.push((turn, request));
    }
    let (tx, rx) = broadcast::channel::<ProxyEvent>(4);
    for (turn, request) in &turns {
        tx.send(ProxyEvent::OperatorTurnClosed {
            closed_turn_id: *turn,
            session_id: String::new(),
            new_request_id: RequestId::from_raw(u64::try_from(*request)?),
            human_prompt: String::new(),
        })
        .map_err(|e| anyhow::anyhow!("send failed: {e}"))?;
    }
    let shutdown = CancellationToken::new();
    let deps = DossierWorkerDeps {
        persist: f.handle.clone(),
        toggles: Arc::new(ArcSwap::from_pointee(FeatureToggles {
            dossier_builder: true,
            ..FeatureToggles::default()
        })),
        shutdown: shutdown.clone(),
    };
    let worker = tokio::spawn(run_dossier_builder(rx, deps));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    let mut built = 0_i64;
    while tokio::time::Instant::now() < deadline {
        built = sqlx::query_scalar("SELECT count(*) FROM memory.turn_facts")
            .fetch_one(f.handle.pool())
            .await?;
        if built == i64::try_from(SESSIONS)? {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    shutdown.cancel();
    worker.await?;
    assert_eq!(
        built,
        i64::try_from(SESSIONS)?,
        "every seeded turn must have facts after one backstop interval"
    );
    f.db.cleanup().await?;
    Ok(())
}
