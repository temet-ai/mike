use mike::store::persistence::steering_evaluations::{
    EvaluationFilter, EvaluationLabel, EvaluationMode, EvaluationStatsFilter, EvaluationVerdict,
    NewEvaluation,
};

use crate::store::operator_turns::support::{OpenTurnFixture, seed_open_turn_fixture};

async fn steerer_id(f: &OpenTurnFixture, kind: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = $1 ORDER BY id LIMIT 1")
            .bind(kind)
            .fetch_one(&f.pool)
            .await?,
    )
}

async fn session_db_id(f: &OpenTurnFixture) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&f.session)
            .fetch_one(&f.pool)
            .await?,
    )
}

async fn new_evaluation(
    f: &OpenTurnFixture,
    kind: &str,
    mode: EvaluationMode,
) -> anyhow::Result<NewEvaluation<'static>> {
    Ok(NewEvaluation {
        steerer_id: steerer_id(f, kind).await?,
        session_id: session_db_id(f).await?,
        operator_turn_id: f.turn_id,
        mode,
        bundle_id: None,
        trigger_point: Some("turn_end"),
        trigger_request_id: Some(f.request_id),
    })
}

async fn resolved(
    f: &OpenTurnFixture,
    mode: EvaluationMode,
    fired: bool,
) -> anyhow::Result<uuid::Uuid> {
    let new = new_evaluation(f, "verifier", mode).await?;
    let uuid = f.handle.insert_evaluation_dispatched(&new).await?;
    let verdict = EvaluationVerdict {
        verdict_json: serde_json::json!({"fires": fired, "reason": "a × b = ç", "nested": {"k": [1, 2]}}),
        confidence: Some(0.8),
        reason: Some("because".to_owned()),
    };
    f.handle
        .resolve_evaluation(uuid, fired, None, Some(verdict))
        .await?;
    Ok(uuid)
}

async fn done(f: OpenTurnFixture) -> anyhow::Result<()> {
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn insert_evaluation_dispatched_with_steerer_id_writes_one_row_for_that_steerer()
-> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let new = new_evaluation(&f, "verifier", EvaluationMode::Live).await?;
    f.handle.insert_evaluation_dispatched(&new).await?;
    let rows: Vec<(i64, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT steerer_id, mode, trigger_point, trigger_request_id FROM steering.evaluations WHERE operator_turn_id = $1",
    )
    .bind(f.turn_id)
    .fetch_all(&f.pool)
    .await?;
    assert_eq!(
        rows,
        vec![(
            new.steerer_id,
            "live".to_owned(),
            Some("turn_end".to_owned()),
            Some(f.request_id)
        )]
    );
    done(f).await
}

#[tokio::test]
async fn insert_evaluations_dispatched_writes_one_row_per_question_in_one_statement()
-> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let new = new_evaluation(&f, "verifier", EvaluationMode::Shadow).await?;
    let questions = vec![Some("relapse".to_owned()), Some("unbacked_wait".to_owned())];
    let rows = f
        .handle
        .insert_evaluations_dispatched(&new, &questions)
        .await?;
    let mut got: Vec<Option<String>> = rows.into_iter().map(|(_, q)| q).collect();
    got.sort();
    assert_eq!(got, questions);
    done(f).await
}

#[tokio::test]
async fn resolve_evaluation_shadow_would_fire_has_no_intervention_uuid() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let uuid = resolved(&f, EvaluationMode::Shadow, true).await?;
    let (status, intervention): (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT status, intervention_uuid FROM steering.evaluations WHERE uuid = $1",
    )
    .bind(uuid)
    .fetch_one(&f.pool)
    .await?;
    assert_eq!(status, "resolved_correction");
    assert!(intervention.is_none());
    done(f).await
}

async fn delivered_note(
    f: &OpenTurnFixture,
    new: &NewEvaluation<'_>,
) -> anyhow::Result<uuid::Uuid> {
    let note = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO steering.interventions (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid,
                                             expires_at, payload_text)
         SELECT $1, $2, v.id, m.id, a.id, $3, now() + interval '1 hour', 'note'
           FROM steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE v.code = 'fired' AND m.code = 'drive' AND a.code = 'command'",
    )
    .bind(new.steerer_id)
    .bind(new.session_id)
    .bind(note)
    .execute(&f.pool)
    .await?;
    Ok(note)
}

#[tokio::test]
async fn auto_label_on_a_live_row_whose_note_was_delivered_is_rejected_by_check()
-> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let new = new_evaluation(&f, "verifier", EvaluationMode::Live).await?;
    let uuid = f.handle.insert_evaluation_dispatched(&new).await?;
    let note = delivered_note(&f, &new).await?;
    f.handle
        .resolve_evaluation(uuid, true, Some(note), None)
        .await?;
    let err = sqlx::query(
        "UPDATE steering.evaluations SET outcome_label = 'right', label_source = 'next_message',
                labelled_at = NOW(), label_evidence = '{}'::jsonb WHERE mode = 'live'",
    )
    .execute(&f.pool)
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(err.contains("evaluations_auto_label_shadow_only"), "{err}");
    done(f).await
}

#[tokio::test]
async fn label_columns_all_or_none_check_rejects_partial_label() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    resolved(&f, EvaluationMode::Shadow, true).await?;
    let err = sqlx::query("UPDATE steering.evaluations SET outcome_label = 'right'")
        .execute(&f.pool)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("evaluations_label_all_or_none"), "{err}");
    done(f).await
}

#[tokio::test]
async fn label_on_dispatched_row_is_rejected_by_check() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let new = new_evaluation(&f, "verifier", EvaluationMode::Shadow).await?;
    f.handle.insert_evaluation_dispatched(&new).await?;
    let err = sqlx::query(
        "UPDATE steering.evaluations SET outcome_label = 'right', label_source = 'operator',
                labelled_at = NOW(), label_evidence = '{}'::jsonb",
    )
    .execute(&f.pool)
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(err.contains("evaluations_label_needs_resolution"), "{err}");
    done(f).await
}

fn label(uuid: uuid::Uuid, value: &str, source: &str) -> EvaluationLabel {
    EvaluationLabel {
        uuid,
        label: value.to_owned(),
        source: source.to_owned(),
        evidence: serde_json::json!({"source": source}),
    }
}

async fn label_of(
    f: &OpenTurnFixture,
    uuid: uuid::Uuid,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT outcome_label, label_source FROM steering.evaluations WHERE uuid = $1",
    )
    .bind(uuid)
    .fetch_one(&f.pool)
    .await?)
}

#[tokio::test]
async fn apply_evaluation_labels_operator_overrides_next_message() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let uuid = resolved(&f, EvaluationMode::Shadow, true).await?;
    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[label(uuid, "right", "next_message")])
            .await?,
        1
    );
    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[label(uuid, "wrong", "operator")])
            .await?,
        1
    );
    assert_eq!(
        label_of(&f, uuid).await?,
        (Some("wrong".to_owned()), Some("operator".to_owned()))
    );
    done(f).await
}

#[tokio::test]
async fn apply_evaluation_labels_next_message_never_overrides_operator() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let uuid = resolved(&f, EvaluationMode::Shadow, true).await?;
    f.handle
        .apply_evaluation_labels(&[label(uuid, "wrong", "operator")])
        .await?;
    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[label(uuid, "right", "next_message")])
            .await?,
        0
    );
    assert_eq!(
        label_of(&f, uuid).await?,
        (Some("wrong".to_owned()), Some("operator".to_owned()))
    );
    done(f).await
}

fn filter(
    f: &OpenTurnFixture,
    limit: i64,
    before_id: Option<i64>,
) -> anyhow::Result<EvaluationFilter> {
    Ok(EvaluationFilter {
        session_uuid: f.session.parse()?,
        limit,
        before_id,
        ..EvaluationFilter::default()
    })
}

#[tokio::test]
async fn list_session_evaluations_pages_complete_rows_by_before_id() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    for _ in 0..5 {
        resolved(&f, EvaluationMode::Live, false).await?;
    }
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let (rows, total) = f
            .handle
            .list_session_evaluations(&filter(&f, 2, cursor)?)
            .await?;
        assert_eq!(total, 5);
        if rows.is_empty() {
            break;
        }
        cursor = rows.last().map(|r| r.id);
        seen.extend(rows.into_iter().map(|r| r.id));
    }
    let mut unique = seen.clone();
    unique.dedup();
    assert_eq!(seen.len(), 5);
    assert_eq!(unique.len(), 5);
    done(f).await
}

#[tokio::test]
async fn list_session_evaluations_returns_verdict_json_byte_identical() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let uuid = resolved(&f, EvaluationMode::Live, false).await?;
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT verdict_json FROM steering.evaluations WHERE uuid = $1")
            .bind(uuid)
            .fetch_one(&f.pool)
            .await?;
    let (rows, _) = f
        .handle
        .list_session_evaluations(&filter(&f, 50, None)?)
        .await?;
    assert_eq!(rows[0].verdict_json.as_ref(), Some(&stored));
    done(f).await
}

#[tokio::test]
async fn evaluation_stats_counts_match_list_under_same_filter() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    resolved(&f, EvaluationMode::Shadow, true).await?;
    resolved(&f, EvaluationMode::Shadow, false).await?;
    resolved(&f, EvaluationMode::Live, false).await?;
    let stats = f
        .handle
        .evaluation_stats(&EvaluationStatsFilter {
            mode: Some("shadow".to_owned()),
            ..EvaluationStatsFilter::default()
        })
        .await?;
    let shadow_listed = f
        .handle
        .list_session_evaluations(&EvaluationFilter {
            mode: Some("shadow".to_owned()),
            ..filter(&f, 200, None)?
        })
        .await?
        .1;
    let counted: i64 = stats.iter().map(|s| s.evaluations).sum();
    assert_eq!(counted, shadow_listed);
    assert_eq!(stats.iter().map(|s| s.fires).sum::<i64>(), 1);
    done(f).await
}

#[tokio::test]
async fn get_latest_evaluation_for_session_ignores_shadow_and_non_verifier_rows()
-> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let live = resolved(&f, EvaluationMode::Live, false).await?;
    resolved(&f, EvaluationMode::Shadow, true).await?;
    let latest = f
        .handle
        .get_latest_evaluation_for_session(f.session.parse()?, "verifier")
        .await?;
    assert_eq!(latest.map(|r| r.uuid), Some(live));
    done(f).await
}

#[tokio::test]
async fn get_turn_for_trigger_request_uses_request_turn_when_stamped() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2")
        .bind(f.turn_id)
        .bind(f.request_id)
        .execute(&f.pool)
        .await?;
    let turn = f
        .handle
        .get_turn_for_trigger_request(f.session.parse()?, f.request_id)
        .await?;
    assert_eq!(turn.and_then(|t| t.operator_turn_id), Some(f.turn_id));
    done(f).await
}

#[tokio::test]
async fn get_turn_for_trigger_request_falls_back_to_latest_earlier_turn() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE requests.requests SET operator_turn_id = NULL, created_at = NOW() + INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(f.request_id)
    .execute(&f.pool)
    .await?;
    let turn = f
        .handle
        .get_turn_for_trigger_request(f.session.parse()?, f.request_id)
        .await?;
    assert_eq!(turn.and_then(|t| t.operator_turn_id), Some(f.turn_id));
    done(f).await
}

#[tokio::test]
async fn get_turn_for_trigger_request_none_when_session_has_no_turn() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    sqlx::query("UPDATE requests.requests SET operator_turn_id = NULL WHERE id = $1")
        .bind(f.request_id)
        .execute(&f.pool)
        .await?;
    sqlx::query("DELETE FROM memory.operator_turns WHERE id = $1")
        .bind(f.turn_id)
        .execute(&f.pool)
        .await?;
    let turn = f
        .handle
        .get_turn_for_trigger_request(f.session.parse()?, f.request_id)
        .await?;
    assert_eq!(turn.and_then(|t| t.operator_turn_id), None);
    done(f).await
}
