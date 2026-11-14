use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::sync::atomic::{AtomicUsize, Ordering};

use uuid::Uuid;

use super::schema::{AdjudicatorVerdict, ProposerVerdict};
use super::{RelationJudge, run_detection, run_detection_on_reranked};
use crate::llm::LlmError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

struct CountingJudge {
    proposals: AtomicUsize,
}

impl RelationJudge for CountingJudge {
    async fn propose(&self, _prompt: &str) -> Result<ProposerVerdict, LlmError> {
        self.proposals.fetch_add(1, Ordering::SeqCst);
        Ok(ProposerVerdict {
            is_related: false,
            kind: "refines".to_owned(),
            from_lesson_id: 0,
            to_lesson_id: 0,
            rationale: "unrelated".to_owned(),
            quoted_evidence: Vec::new(),
        })
    }

    async fn adjudicate(&self, _prompt: &str) -> Result<AdjudicatorVerdict, LlmError> {
        Ok(AdjudicatorVerdict {
            holds: true,
            from_lesson_id: 0,
            to_lesson_id: 0,
            rationale: "unused".to_owned(),
        })
    }

    fn proposer_model(&self) -> &str {
        "stub/proposer"
    }

    fn adjudicator_model(&self) -> &str {
        "stub/adjudicator"
    }
}

fn judge() -> CountingJudge {
    CountingJudge {
        proposals: AtomicUsize::new(0),
    }
}

async fn queued_pair(handle: &PersistHandle) -> anyhow::Result<(i64, i64, i64)> {
    let project = handle
        .knowledge()
        .define_knowledge_project("reranked pairs", Uuid::new_v4())
        .await?
        .id;
    let ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, t.text, ls.id, lsrc.id, 'checks'
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc,
                (VALUES ('Run the narrow check before committing', 1),
                        ('Commit only after one narrow check has passed', 2)) AS t(text, ord)
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'
          ORDER BY t.ord
      RETURNING id",
    )
    .bind(project)
    .fetch_all(handle.pool())
    .await?;
    let [a, b] = ids.as_slice() else {
        anyhow::bail!("two lessons expected, got {}", ids.len());
    };
    sqlx::query(
        "UPDATE memory.lessons
            SET embedding = array_fill(0.1::real, ARRAY[(
                    SELECT atttypmod FROM pg_attribute
                     WHERE attrelid = 'memory.lessons'::regclass AND attname = 'embedding')])::vector,
                embedded_at = now(), embedding_model = 'test'
          WHERE id = ANY($1)",
    )
    .bind(&ids)
    .execute(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO memory.lesson_pair_candidates (lesson_a_id, lesson_b_id, score, distance)
         VALUES ($1, $2, 0.9, 0.3)",
    )
    .bind(a.min(b))
    .bind(a.max(b))
    .execute(handle.pool())
    .await?;
    Ok((project, *a.min(b), *a.max(b)))
}

#[tokio::test]
async fn reranked_pair_reaches_the_judge_and_is_marked_judged() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (project, a, b) = queued_pair(&handle).await?;
    let judge = judge();

    let report = run_detection_on_reranked(&handle, &judge, &[project], false).await?;

    assert_eq!(report.detection.candidate_pairs_considered, 1);
    assert_eq!(report.handled, vec![(a, b)]);
    assert_eq!(judge.proposals.load(Ordering::SeqCst), 1);
    let unjudged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_pair_candidates
          WHERE lesson_a_id = $1 AND judged_at IS NULL",
    )
    .bind(a)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(unjudged, 0);
    let again = run_detection_on_reranked(&handle, &judge, &[project], false).await?;
    assert_eq!(again.detection.candidate_pairs_considered, 0);
    assert_eq!(judge.proposals.load(Ordering::SeqCst), 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reranked_pair_in_a_dry_run_stays_queued() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (project, a, _) = queued_pair(&handle).await?;
    let judge = judge();

    run_detection_on_reranked(&handle, &judge, &[project], true).await?;

    let unjudged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_pair_candidates
          WHERE lesson_a_id = $1 AND judged_at IS NULL",
    )
    .bind(a)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(unjudged, 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn distance_detection_skips_the_pairs_the_reranked_pass_handled() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (project, a, b) = queued_pair(&handle).await?;
    let judge = judge();

    let skipped = run_detection(&handle, &judge, 0.2, &[project], true, &[(a, b)]).await?;
    let seen = run_detection(&handle, &judge, 0.2, &[project], true, &[]).await?;

    assert_eq!(skipped.candidate_pairs_considered, 0);
    assert_eq!(seen.candidate_pairs_considered, 1);
    db.cleanup().await?;
    Ok(())
}
