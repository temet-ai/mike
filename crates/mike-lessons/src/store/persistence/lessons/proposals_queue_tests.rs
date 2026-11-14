use super::{fixture, unit_embedding};
use crate::lesson_proposer::{
    ProposedLesson, ProposerSettings, WriteOutcome, dedupe::Embedded, write,
};
use crate::store::persistence::lessons::proposals::queue::SavedProposal;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistHandle, test_support};

#[tokio::test]
async fn embedding_failure_retains_full_proposal_for_restart_without_new_model_call()
-> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    sqlx::query("UPDATE requests.requests SET human_prompt='The command failed, so verify its exit status before claiming success' WHERE id=(SELECT next_genuine_request_id FROM memory.turn_facts WHERE operator_turn_id=$1)")
        .bind(fx.turn_id).execute(fx.handle.pool()).await?;
    let since = chrono::Utc::now() - chrono::Duration::days(1);
    let candidate = fx
        .handle
        .lessons()
        .select_correction_candidates(since, 1)
        .await?
        .remove(0);
    let saved = SavedProposal {
        candidate,
        proposed: ProposedLesson {
            rule: "Verify command exit status before claiming success".to_owned(),
            kind: "correction".to_owned(),
            generalises: true,
            reason: "reusable".to_owned(),
        },
        similar: serde_json::json!([]),
        model: "test-model".to_owned(),
        cost_usd: Some(0.01),
        generation_id: Some("original-generation".to_owned()),
    };
    fx.handle.lessons().save_lesson_proposal(&saved).await?;
    fx.handle.lessons().save_lesson_proposal(&saved).await?;
    let mut decision = write::Decision {
        prod: false,
        candidate: &saved.candidate,
        proposed: &saved.proposed,
        embedding: None,
        verdict: None,
        model: &saved.model,
        cost_usd: saved.cost_usd,
        generation_id: saved.generation_id.as_deref(),
        similar: &saved.similar,
    };
    assert_eq!(
        write::decide_and_write(&fx.handle, &ProposerSettings::default(), &decision).await?,
        WriteOutcome::AwaitingEmbedding
    );
    assert!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .is_empty()
    );
    let restarted = PersistHandle::connect(&fx.db.database_url).await?;
    let pending = restarted.lessons().pending_lesson_proposals(10).await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].proposed, saved.proposed);
    assert_eq!(pending[0].generation_id, saved.generation_id);
    let embedded = Embedded {
        vector: unit_embedding(),
        model: "test-embedding".to_owned(),
    };
    decision.embedding = Some(&embedded);
    assert!(matches!(
        write::decide_and_write(&restarted, &ProposerSettings::default(), &decision).await?,
        WriteOutcome::Created { .. }
    ));
    assert!(
        restarted
            .lessons()
            .pending_lesson_proposals(10)
            .await?
            .is_empty()
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn daily_budget_is_atomic_and_survives_handle_restart() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    assert!(!handle.lessons().reserve_lesson_proposer_call(0).await?);
    let (first, second) = tokio::join!(
        handle.lessons().reserve_lesson_proposer_call(1),
        handle.lessons().reserve_lesson_proposer_call(1)
    );
    assert_ne!(first?, second?);
    let restarted = PersistHandle::connect(&db.database_url).await?;
    assert!(!restarted.lessons().reserve_lesson_proposer_call(1).await?);
    assert!(restarted.lessons().reserve_lesson_proposer_call(2).await?);
    assert!(!restarted.lessons().reserve_lesson_proposer_call(2).await?);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn proposer_lock_prevents_duplicate_processing_and_releases_on_drop() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let Some(guard) = handle.lessons().lock_lesson_proposer().await? else {
        anyhow::bail!("first worker must lock");
    };
    assert!(handle.lessons().lock_lesson_proposer().await?.is_none());
    guard.rollback().await?;
    let Some(next) = handle.lessons().lock_lesson_proposer().await? else {
        anyhow::bail!("lock must release");
    };
    next.rollback().await?;
    handle.pool().close().await;
    assert!(
        handle
            .lessons()
            .reserve_lesson_proposer_call(10)
            .await
            .is_err()
    );
    assert!(handle.lessons().pending_lesson_proposals(10).await.is_err());
    assert!(handle.lessons().lock_lesson_proposer().await.is_err());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn machine_prompt_and_stale_verdict_are_ineligible_but_empty_reply_is_not()
-> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let since = chrono::Utc::now() - chrono::Duration::days(1);
    sqlx::query("UPDATE memory.operator_turns SET final_assistant_text = '' WHERE id = $1")
        .bind(fx.turn_id)
        .execute(fx.handle.pool())
        .await?;
    assert_eq!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .len(),
        1
    );
    sqlx::query(
        "UPDATE memory.turn_facts SET prompt_kind = 'loop-wakeup' WHERE operator_turn_id = $1",
    )
    .bind(fx.turn_id)
    .execute(fx.handle.pool())
    .await?;
    assert!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .is_empty()
    );
    sqlx::query("UPDATE memory.turn_facts SET prompt_kind = 'genuine' WHERE operator_turn_id = $1")
        .bind(fx.turn_id)
        .execute(fx.handle.pool())
        .await?;
    sqlx::query("UPDATE memory.operator_turns SET live_verdict_id = NULL WHERE id = $1")
        .bind(fx.turn_id)
        .execute(fx.handle.pool())
        .await?;
    assert!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .is_empty()
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_ambiguous_evidence_is_reproposed_once_with_original_preserved() -> anyhow::Result<()>
{
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let empty = serde_json::json!([]);
    fx.handle
        .lessons()
        .record_evidence(super::no_evidence(fx.verdict_id, fx.turn_id, &empty))
        .await?;
    sqlx::query("INSERT INTO memory.lesson_proposal_recovery (verdict_id, original_evidence, recovery_state) SELECT verdict_id, to_jsonb(e), 'needs_reproposal' FROM memory.lesson_proposal_evidence e WHERE verdict_id = $1")
        .bind(fx.verdict_id).execute(fx.handle.pool()).await?;
    let since = chrono::Utc::now() - chrono::Duration::days(1);
    assert_eq!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .len(),
        1
    );
    let mut replacement = super::no_evidence(fx.verdict_id, fx.turn_id, &empty);
    replacement.reason = "new model explicitly rejected generalisation";
    fx.handle.lessons().record_evidence(replacement).await?;
    assert!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .is_empty()
    );
    let (state, original): (String, serde_json::Value) = sqlx::query_as("SELECT recovery_state, original_evidence FROM memory.lesson_proposal_recovery WHERE verdict_id = $1")
        .bind(fx.verdict_id).fetch_one(fx.handle.pool()).await?;
    assert_eq!(state, "reproposed");
    assert_eq!(original["reason"], "test reason");
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn obsolete_legacy_correction_is_terminal_with_original_preserved() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let empty = serde_json::json!([]);
    fx.handle
        .lessons()
        .record_evidence(super::no_evidence(fx.verdict_id, fx.turn_id, &empty))
        .await?;
    sqlx::query("INSERT INTO memory.lesson_proposal_recovery (verdict_id,original_evidence) SELECT verdict_id,to_jsonb(e) FROM memory.lesson_proposal_evidence e WHERE verdict_id=$1")
        .bind(fx.verdict_id).execute(fx.handle.pool()).await?;
    sqlx::query("UPDATE memory.turn_facts SET prompt_kind='loop-wakeup' WHERE operator_turn_id=$1")
        .bind(fx.turn_id)
        .execute(fx.handle.pool())
        .await?;
    assert!(
        fx.handle
            .lessons()
            .pending_lesson_proposals(10)
            .await?
            .is_empty()
    );
    let (state,reason,original): (String,String,serde_json::Value) = sqlx::query_as("SELECT recovery_state,disposition_reason,original_evidence FROM memory.lesson_proposal_recovery WHERE verdict_id=$1")
        .bind(fx.verdict_id).fetch_one(fx.handle.pool()).await?;
    assert_eq!(state, "obsolete");
    assert!(!reason.is_empty());
    assert_eq!(original["reason"], "test reason");
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn quiet_activity_includes_tool_loops_and_excludes_sidecars() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let since = chrono::Utc::now() - chrono::Duration::hours(1);
    assert!(fx.handle.has_recent_primary_activity(since).await?);
    assert!(
        !fx.handle
            .has_recent_primary_activity(chrono::Utc::now() + chrono::Duration::days(1))
            .await?
    );
    sqlx::query("UPDATE requests.requests SET is_tool_loop=true WHERE project_id=$1")
        .bind(fx.project_id)
        .execute(fx.handle.pool())
        .await?;
    assert!(fx.handle.has_recent_primary_activity(since).await?);
    sqlx::query("UPDATE requests.requests SET turn_origin_id=(SELECT id FROM requests.turn_origins WHERE code='title_sidecar') WHERE project_id=$1")
        .bind(fx.project_id).execute(fx.handle.pool()).await?;
    assert!(!fx.handle.has_recent_primary_activity(since).await?);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_unknown_author_requires_positive_operator_classification() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let since = chrono::Utc::now() - chrono::Duration::days(1);
    sqlx::query("UPDATE requests.requests SET author_kind_id=4 WHERE id=(SELECT next_genuine_request_id FROM memory.turn_facts WHERE operator_turn_id=$1)")
        .bind(fx.turn_id).execute(fx.handle.pool()).await?;
    assert_eq!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .len(),
        1
    );
    sqlx::query("UPDATE prompt_kinds.classifications SET kind_id=(SELECT id FROM prompt_kinds.kinds WHERE code='loop-wakeup') WHERE request_id=(SELECT next_genuine_request_id FROM memory.turn_facts WHERE operator_turn_id=$1)")
        .bind(fx.turn_id).execute(fx.handle.pool()).await?;
    assert!(
        fx.handle
            .lessons()
            .select_correction_candidates(since, 10)
            .await?
            .is_empty()
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn refused_limiter_does_not_consume_durable_proposer_allowance() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let limiter = std::sync::Arc::new(crate::llm::limiter::CallLimiter::new(
        1,
        std::time::Duration::from_secs(60),
        100,
        std::time::Duration::from_secs(3600),
    ));
    limiter.exhaust_rate();
    let llm = crate::llm::OpenRouterClient::new_with_limiter(
        String::new(),
        "test-model".to_owned(),
        None,
        None,
        limiter,
    )?
    .with_role(crate::llm::LlmRole::LessonProposer);
    let embedding = crate::analysis::EmbeddingService::new(&crate::analysis::EmbeddingConfig {
        enabled: false,
        url: "http://127.0.0.1:1".to_owned(),
        model: "test".to_owned(),
        chunk_max_chars: 1500,
    });
    let semantic = crate::semantic::SemanticConfig::default();
    let settings = ProposerSettings::default();
    let deps = crate::lesson_proposer::ProposerDeps {
        persist: &fx.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
        decisions: None,
    };
    let report = crate::lesson_proposer::run_tick(&deps).await;
    assert_eq!(report.candidates, 1);
    assert!(report.budget_exhausted);
    assert_eq!(report.errors, 0);
    assert!(fx.handle.lessons().reserve_lesson_proposer_call(1).await?);
    fx.db.cleanup().await?;
    Ok(())
}
