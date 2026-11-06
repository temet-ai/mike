use mike::store::persistence::PersistHandle;
use mike::store::persistence::operator_turns::PendingChunk;
use mike::store::test_support::{TestDb, seed_project, seed_session_turns, seed_two_sessions};

use crate::store::operator_turns::support::unique;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claim_replace_chunks_mark_embedded_chain() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let project_id = seed_project(handle.pool(), &unique("w3-embed-chain")).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &session, &spare, project_id).await?;
    seed_session_turns(handle.pool(), &session, project_id, 1).await?;

    let claimed_ids = handle.fetch_unembedded_turn_ids(10).await?;
    assert_eq!(
        claimed_ids.len(),
        1,
        "exactly the one closed/unembedded turn must be returned — this test needs an \
         otherwise-empty database, so it keeps acquire_isolated"
    );
    let turn_id = claimed_ids[0];

    let vec_a = {
        let mut v = vec![0.0_f32; 768];
        v[0] = 1.0;
        v
    };
    let vec_b = {
        let mut v = vec![0.0_f32; 768];
        v[1] = 1.0;
        v
    };
    let plan_two = vec![
        PendingChunk {
            chunk_idx: 0,
            chunk_role: "prompt",
            chunk_text: "first chunk".to_string(),
        },
        PendingChunk {
            chunk_idx: 1,
            chunk_role: "assistant",
            chunk_text: "second chunk".to_string(),
        },
    ];
    handle
        .replace_turn_chunks(
            turn_id,
            &plan_two,
            &[vec_a.clone(), vec_b.clone()],
            "bge-small-en-v1.5",
        )
        .await?;
    let count_after_two: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.operator_turn_chunks WHERE turn_id = $1")
            .bind(turn_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(count_after_two, 2, "two chunks must be written");

    let plan_one = vec![PendingChunk {
        chunk_idx: 0,
        chunk_role: "prompt",
        chunk_text: "replacement".to_string(),
    }];
    handle
        .replace_turn_chunks(
            turn_id,
            &plan_one,
            std::slice::from_ref(&vec_a),
            "bge-small-en-v1.5",
        )
        .await?;
    let count_after_one: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.operator_turn_chunks WHERE turn_id = $1")
            .bind(turn_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        count_after_one, 1,
        "idempotent replace must leave exactly the new plan"
    );

    handle.mark_turns_embedded(&[turn_id]).await?;
    let reclaim = handle.fetch_unembedded_turn_ids(10).await?;
    assert!(
        reclaim.is_empty(),
        "after mark_turns_embedded the turn must not be re-fetched; got {reclaim:?}"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
