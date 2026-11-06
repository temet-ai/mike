use mike::store::persistence::PersistHandle;
use mike::store::test_support;

async fn seed_batch(handle: &PersistHandle, key: &str, turns: &[i64]) -> anyhow::Result<()> {
    let batch: i64 = sqlx::query_scalar(
        "INSERT INTO eval.import_batches (source_key, manifest, counts, imported_by)
         VALUES ($1, '{}'::jsonb, '{}'::jsonb, 'test') RETURNING id",
    )
    .bind(key)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         SELECT id, 'holdout', 'holdout_import' FROM unnest($1::bigint[]) AS t(id) ON CONFLICT DO NOTHING",
    )
    .bind(turns)
    .execute(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                                  label, tier, method, split, source, import_batch_id)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', 'accepted', 'seed',
                'double_blind_adjudicated', 'holdout', $2, $3
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = ANY($1)",
    )
    .bind(turns)
    .bind(format!("jc11/{key}"))
    .bind(batch)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn current(handle: &PersistHandle, key: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM eval.labels_as_of(now()) l JOIN eval.import_batches b ON b.id = l.import_batch_id
          WHERE b.source_key = $1",
    )
    .bind(key)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn withdraw_batch_withdraws_one_batch_and_leaves_another_untouched() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_support::seed_project(handle.pool(), "jc11-withdraw").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(handle.pool(), &session, &spare, project).await?;
    test_support::seed_session_turns(handle.pool(), &session, project, 12).await?;
    let turns: Vec<i64> = sqlx::query_scalar("SELECT id FROM memory.operator_turns ORDER BY id")
        .fetch_all(handle.pool())
        .await?;
    seed_batch(&handle, "jc11-test", &turns[..10]).await?;
    seed_batch(&handle, "other", &turns[10..]).await?;

    let dry = handle
        .withdraw_label_batch("jc11-test", "test", "tester", false)
        .await?;
    assert_eq!((dry.labels, dry.executed), (10, false));
    assert_eq!(current(&handle, "jc11-test").await?, 10);

    let done = handle
        .withdraw_label_batch("jc11-test", "test", "tester", true)
        .await?;
    assert_eq!(done.labels, 10);
    assert_eq!(current(&handle, "jc11-test").await?, 0);
    assert_eq!(current(&handle, "other").await?, 2);

    let again = handle
        .withdraw_label_batch("jc11-test", "test", "tester", true)
        .await?;
    assert_eq!(again.labels, 0);
    db.cleanup().await?;
    Ok(())
}
