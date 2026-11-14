use mike::lesson_proposer::dedupe::Embedded;
use mike::lesson_proposer::placement::Placement;
use mike::lesson_proposer::prompt::ProposedLesson;
use mike::lesson_proposer::relate::{WriteAction, WriteTimeVerdict};
use mike::lesson_proposer::write::{Decision, WriteOutcome, decide_and_write};
use mike::lesson_proposer::{ProposerSettings, run_tick};
use mike::store::persistence::lessons::write_time::PlannedRelation;
use serde_json::json;

use crate::turn_facts::fixture;

use super::{seed_corrected_turn, seed_corrected_turn_with_confidence};

fn settings(auto_promote_min_evidence: i64) -> ProposerSettings {
    ProposerSettings {
        auto_promote_min_evidence,
        ..ProposerSettings::default()
    }
}

fn proposed(rule: &str, generalises: bool) -> ProposedLesson {
    let Ok(lesson) = serde_json::from_value(json!({
        "rule": rule,
        "kind": "correction",
        "generalises": generalises,
        "reason": "test",
    })) else {
        panic!("valid ProposedLesson JSON");
    };
    lesson
}

fn embedded() -> Embedded {
    Embedded {
        vector: vec![0.01_f32; 768],
        model: "test-embedder".to_owned(),
    }
}

async fn evidence_count(pool: &sqlx::PgPool, verdict_id: i64) -> i64 {
    let Ok(count) = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_proposal_evidence WHERE verdict_id = $1",
    )
    .bind(verdict_id)
    .fetch_one(pool)
    .await
    else {
        panic!("evidence count query");
    };
    count
}

#[tokio::test]
async fn evidence_only_when_the_model_finds_no_generalising_rule() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn(&f, 40_100).await?;
    let empty = json!([]);
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed("", false),
        embedding: None,
        verdict: None,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &settings(2), &decision).await?;
    assert_eq!(outcome, WriteOutcome::EvidenceOnly);
    assert_eq!(
        evidence_count(f.handle.pool(), candidate.verdict_id).await,
        1
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn evidence_only_when_the_correcting_message_is_a_prod() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut candidate = seed_corrected_turn(&f, 40_200).await?;
    candidate.correcting_message = Some("?".to_owned());
    let empty = json!([]);
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed("always run the tests before claiming they pass", true),
        embedding: Some(&embedded()),
        verdict: None,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &settings(2), &decision).await?;
    assert_eq!(outcome, WriteOutcome::EvidenceOnly);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn evidence_only_when_the_verdict_confidence_is_below_the_threshold() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut candidate = seed_corrected_turn(&f, 40_150).await?;
    candidate.confidence = 0.5;
    let embedded = embedded();
    let empty = json!([]);
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed("always run the tests before claiming they pass", true),
        embedding: Some(&embedded),
        verdict: None,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let settings = ProposerSettings {
        min_confidence: 0.7,
        ..ProposerSettings::default()
    };
    let outcome = decide_and_write(&f.handle, &settings, &decision).await?;
    assert_eq!(outcome, WriteOutcome::EvidenceOnly);
    assert_eq!(
        evidence_count(f.handle.pool(), candidate.verdict_id).await,
        1
    );
    let lesson_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.lessons WHERE project_id = $1")
            .bind(candidate.project_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(lesson_count, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn creates_a_raw_lesson_and_leaves_it_pending_below_the_promotion_threshold()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn(&f, 40_300).await?;
    let embedded = embedded();
    let empty = json!([]);
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed("always run the tests before claiming they pass", true),
        embedding: Some(&embedded),
        verdict: None,
        model: "test-model",
        cost_usd: Some(0.001),
        generation_id: Some("gen-1"),
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &settings(2), &decision).await?;
    let WriteOutcome::Created {
        lesson_id,
        promoted,
    } = outcome
    else {
        anyhow::bail!("expected Created, got {outcome:?}");
    };
    assert!(!promoted);
    let (stage, source, text): (String, String, String) = sqlx::query_as(
        "SELECT ls.code, lsrc.code, l.text
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(stage, "raw");
    assert_eq!(source, "judge");
    assert_eq!(text, "always run the tests before claiming they pass");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn promotes_the_lesson_once_evidence_reaches_the_configured_minimum() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn(&f, 40_400).await?;
    let embedded = embedded();
    let empty = json!([]);
    let decision = Decision {
        candidate: &candidate,
        proposed: &proposed("always run the tests before claiming they pass", true),
        embedding: Some(&embedded),
        verdict: None,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &settings(1), &decision).await?;
    let WriteOutcome::Created { promoted, .. } = outcome else {
        anyhow::bail!("expected Created, got {outcome:?}");
    };
    assert!(promoted);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reinforces_an_existing_lesson_instead_of_writing_a_new_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = seed_corrected_turn(&f, 40_500).await?;
    let embedded = embedded();
    let empty = json!([]);
    let first_decision = Decision {
        candidate: &first,
        proposed: &proposed("always run the tests before claiming they pass", true),
        embedding: Some(&embedded),
        verdict: None,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let WriteOutcome::Created { lesson_id, .. } =
        decide_and_write(&f.handle, &settings(5), &first_decision).await?
    else {
        anyhow::bail!("expected Created for the first candidate");
    };

    let second = seed_corrected_turn(&f, 40_510).await?;
    let skip = WriteTimeVerdict {
        action: WriteAction::Skip { target: lesson_id },
        relations: vec![PlannedRelation {
            to_lesson_id: lesson_id,
            kind: "recurs".to_owned(),
            distance: 0.02,
            rationale: "same rule, worded differently".to_owned(),
            evidence: json!({"decision": "skip"}),
        }],
        placement: Placement::memory("a fact"),
        notes: Vec::new(),
    };
    let second_decision = Decision {
        candidate: &second,
        proposed: &proposed("run the tests before saying they pass", true),
        embedding: Some(&embedded),
        verdict: Some(&skip),
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        similar: &empty,
    };
    let outcome = decide_and_write(&f.handle, &settings(5), &second_decision).await?;
    assert_eq!(
        outcome,
        WriteOutcome::Reinforced {
            lesson_id,
            promoted: false
        }
    );
    let active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lessons WHERE project_id = $1 AND archived_at IS NULL",
    )
    .bind(first.project_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(active_count, 1);
    let (reason, duplicate_of, text): (String, i64, String) = sqlx::query_as(
        "SELECT ar.code, l.duplicate_of_id, l.text
           FROM memory.lessons l
           JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.project_id = $1 AND l.archived_at IS NOT NULL",
    )
    .bind(first.project_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(reason, "duplicate");
    assert_eq!(duplicate_of, lesson_id);
    assert_eq!(text, "run the tests before saying they pass");
    let (kind, status, method): (String, String, String) = sqlx::query_as(
        "SELECT k.code, s.code, m.code
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
           JOIN memory.lesson_relation_statuses s ON s.id = r.status_id
           JOIN memory.lesson_relation_methods m ON m.id = r.method_id
          WHERE r.to_lesson_id = $1",
    )
    .bind(lesson_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        (kind.as_str(), status.as_str(), method.as_str()),
        ("recurs", "proposed", "write_time")
    );
    let consolidation_count: i32 =
        sqlx::query_scalar("SELECT consolidation_count FROM memory.lessons WHERE id = $1")
            .bind(lesson_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(consolidation_count, 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn run_tick_examines_nothing_when_no_candidates_are_pending() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let llm = mike::llm::OpenRouterClient::new(String::new(), "test-model".to_owned(), None, None)?;
    let embedding = mike::analysis::EmbeddingService::new(&mike::analysis::EmbeddingConfig {
        enabled: false,
        url: "http://127.0.0.1:7997".to_owned(),
        model: "nomic-embed-text-v1.5".to_owned(),
        chunk_max_chars: 1500,
    });
    let semantic = mike::semantic::SemanticConfig::default();
    let settings = settings(2);
    let deps = mike::lesson_proposer::ProposerDeps {
        persist: &f.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
        decisions: None,
    };
    let report = run_tick(&deps).await;
    assert_eq!(report.candidates, 0);
    assert_eq!(report.errors, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn run_tick_skips_the_llm_for_a_below_threshold_candidate() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let candidate = seed_corrected_turn_with_confidence(&f, 40_600, 0.5).await?;
    let llm = mike::llm::OpenRouterClient::new(String::new(), "test-model".to_owned(), None, None)?;
    let embedding = mike::analysis::EmbeddingService::new(&mike::analysis::EmbeddingConfig {
        enabled: false,
        url: "http://127.0.0.1:7997".to_owned(),
        model: "nomic-embed-text-v1.5".to_owned(),
        chunk_max_chars: 1500,
    });
    let semantic = mike::semantic::SemanticConfig::default();
    let settings = ProposerSettings {
        min_confidence: 0.7,
        ..settings(2)
    };
    let deps = mike::lesson_proposer::ProposerDeps {
        persist: &f.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
        decisions: None,
    };
    let report = run_tick(&deps).await;
    assert_eq!(report.candidates, 1);
    assert_eq!(
        report.errors, 0,
        "an LLM call against the empty base URL would fail and show up here, proving it was skipped"
    );
    assert_eq!(report.evidence_only, 1);
    assert_eq!(
        evidence_count(f.handle.pool(), candidate.verdict_id).await,
        1
    );
    f.db.cleanup().await?;
    Ok(())
}
