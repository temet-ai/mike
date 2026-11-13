use chrono::{Duration, Utc};
use mike::dossier::view::types::AsOf;
use mike::semantic::SemanticConfig;
use mike::semantic::neighbours::Retriever;
use mike::semantic::retrieve::{Audience, QuerySubject, RetrievedKind, Scope, SemanticQuery};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};

use super::support::{
    build_all, closed_turn, embedder, embedding_stub, facts, floors_in_force, label, service,
};
use crate::turn_facts::{
    Fixture, NewTurn, Req, fixture, insert_request_in, insert_session, insert_turn_in,
};

const CORRECTION: &str = "you broke the parser again, run the parser tests before you say done";

struct Scene {
    predecessor: i64,
    correction: i64,
    subject: i64,
}

async fn subject_in_other_session(f: &Fixture) -> anyhow::Result<i64> {
    let other = insert_session(f.handle.pool(), f.project_id, Utc::now()).await?;
    insert_request_in(f, other, &Req::primary(30_210, 300, CORRECTION)).await?;
    let turn = insert_turn_in(f, other, &NewTurn::closed(30_210, 30_210, 300, 320)).await?;
    sqlx::query("UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = 'Looking at it.' WHERE id = $1")
        .bind(turn)
        .bind(CORRECTION)
        .execute(f.handle.pool())
        .await?;
    let mut row = crate::turn_facts::sample_row(turn, other, Utc::now());
    crate::turn_facts::resolve(&mut row);
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(turn)
}

async fn scene(f: &Fixture) -> anyhow::Result<(Scene, String)> {
    let stub = embedding_stub().await;
    let svc = service(&stub.uri());
    let predecessor = closed_turn(f, 30_200, "fix the parser", 0).await?;
    let correction = closed_turn(f, 30_201, CORRECTION, 100).await?;
    facts(f, predecessor, Some(correction)).await?;
    facts(f, correction, None).await?;
    let subject = subject_in_other_session(f).await?;
    let e = embedder(f, &svc).await?;
    build_all(f, &svc, &e).await?;
    floors_in_force(f, &e.embedder_key, 0.5).await?;
    Ok((
        Scene {
            predecessor,
            correction,
            subject,
        },
        e.embedder_key,
    ))
}

async fn found(f: &Fixture, scene: &Scene, audience: Audience) -> anyhow::Result<bool> {
    let config = SemanticConfig::default();
    let retriever = Retriever {
        persist: &f.handle,
        embedding: None,
        config: &config,
        live_consumer: false,
    };
    let set = retriever
        .neighbours(&SemanticQuery {
            subject: QuerySubject::ClosedTurn(scene.subject),
            audience,
            kinds: vec![RetrievedKind::Correction],
            scope: Scope::Operator,
            as_of: AsOf::knowledge(Utc::now() + Duration::hours(1)),
            k: 5,
            offset: 0,
        })
        .await?;
    Ok(set
        .neighbours
        .iter()
        .any(|n| n.row.operator_turn_id == Some(scene.correction)))
}

async fn live_verdict(f: &Fixture, turn: i64) -> anyhow::Result<()> {
    let id = f
        .handle
        .eval()
        .insert_turn_verdict(TurnVerdictInsert {
            turn_id: turn,
            mode: VerdictMode::Live,
            bundle_id: None,
            rubric_version: "v2-2026-09-23",
            input_builder_version: "judge-input-v2.1",
            slices: Vec::new(),
            model: "google/gemini-3.5-flash",
            judgement: VerdictJudgement {
                shape: VerdictShape::TwoAxis {
                    reaction: "corrected",
                    delivery: "partial",
                    failure_tags: None,
                    severity: None,
                },
                confidence: 0.9,
                rationale: "corrected",
                unmet_ask: "",
                evidence_quality: "asserted",
            },
            next_genuine_request_id: None,
            as_of: Utc::now(),
            input_text: "x",
            usage: VerdictCallUsage::default(),
        })
        .await?;
    sqlx::query("UPDATE memory.turn_verdicts SET projected = true WHERE id = $1")
        .bind(id)
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn retrieval_judge_audience_ignores_live_verdict_corrections() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (s, _) = scene(&f).await?;
    assert!(
        !found(&f, &s, Audience::Steerer).await?,
        "no qualification yet"
    );
    live_verdict(&f, s.predecessor).await?;
    assert!(found(&f, &s, Audience::Steerer).await?);
    assert!(!found(&f, &s, Audience::Judge).await?);
    Ok(())
}

#[tokio::test]
async fn retrieval_seed_tier_qualifies_for_internal_only() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (s, _) = scene(&f).await?;
    label(&f, s.predecessor, "seed").await?;
    assert!(found(&f, &s, Audience::Internal).await?);
    assert!(!found(&f, &s, Audience::Steerer).await?);
    assert!(!found(&f, &s, Audience::Judge).await?);
    Ok(())
}

#[tokio::test]
async fn retrieval_never_returns_holdout_turn_or_its_predecessor() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (s, _) = scene(&f).await?;
    label(&f, s.predecessor, "gold").await?;
    assert!(found(&f, &s, Audience::Judge).await?);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DELETE FROM eval.labels WHERE operator_turn_id = {turn};
         UPDATE eval.turn_splits SET split = 'holdout' WHERE operator_turn_id = {turn};
         INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid,
                                  axis, label, tier, method, split, source)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', 'corrected', 'gold',
                'operator_click', 'holdout', 'test'
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = {turn};",
        turn = s.predecessor
    )))
    .execute(f.handle.pool())
    .await?;
    assert!(!found(&f, &s, Audience::Judge).await?);
    assert!(!found(&f, &s, Audience::Internal).await?);
    Ok(())
}

#[tokio::test]
async fn retrieval_as_of_ignores_label_written_after_as_of() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (s, _) = scene(&f).await?;
    let before = Utc::now();
    label(&f, s.predecessor, "gold").await?;
    let config = SemanticConfig::default();
    let retriever = Retriever {
        persist: &f.handle,
        embedding: None,
        config: &config,
        live_consumer: true,
    };
    let set = retriever
        .neighbours(&SemanticQuery {
            subject: QuerySubject::ClosedTurn(s.subject),
            audience: Audience::Judge,
            kinds: vec![RetrievedKind::Correction],
            scope: Scope::Operator,
            as_of: AsOf::knowledge(before),
            k: 5,
            offset: 0,
        })
        .await?;
    assert!(
        set.neighbours
            .iter()
            .all(|n| n.row.operator_turn_id != Some(s.correction))
    );
    Ok(())
}
