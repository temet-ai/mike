use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::sync::Mutex;

use chrono::{Duration, Utc};

use super::prompt::Answer;
use super::{JudgeError, SupplyJudge, judge_batch};
use crate::llm::LlmError;
use crate::store::persistence::supplied_items::tests::Fixture;
use crate::store::persistence::supplied_items::{PendingVerdict, SuppliedLesson};

struct FakeJudge {
    answers: Mutex<Vec<Result<Answer, JudgeError>>>,
    seen: Mutex<Vec<String>>,
    without_model: bool,
}

impl FakeJudge {
    fn new(answers: Vec<Result<Answer, JudgeError>>) -> Self {
        Self {
            answers: Mutex::new(answers),
            seen: Mutex::new(Vec::new()),
            without_model: false,
        }
    }

    fn deciding_without_model(answers: Vec<Result<Answer, JudgeError>>) -> Self {
        Self {
            without_model: true,
            ..Self::new(answers)
        }
    }
}

impl SupplyJudge for FakeJudge {
    fn model(&self) -> &str {
        "fake-judge"
    }

    fn decides_without_model(&self) -> bool {
        self.without_model
    }

    async fn judge(&self, _item: &PendingVerdict, user: &str) -> Result<Answer, JudgeError> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(user.to_owned());
        }
        self.answers
            .lock()
            .map_err(|_| JudgeError::Failed("poisoned".to_owned()))?
            .pop()
            .unwrap_or_else(|| Err(JudgeError::Failed("no answer left".to_owned())))
    }
}

fn answer(verdict: &str) -> Result<Answer, JudgeError> {
    Ok(Answer {
        verdict: verdict.to_owned(),
        rationale: format!("judged {verdict}"),
        evidence: String::new(),
    })
}

#[tokio::test]
async fn judge_batch_writes_one_verdict_per_supplied_item_and_skips_bad_answers()
-> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET user_prompt = 'fix the flaky test',
                final_assistant_text = 'Ran the narrow test first, it passes.'",
    )
    .execute(f.handle.pool())
    .await?;
    let lessons = [
        SuppliedLesson {
            lesson_id: f.lessons[0],
            route: "mod",
            score: Some(0.9),
        },
        SuppliedLesson {
            lesson_id: f.lessons[1],
            route: "mod",
            score: Some(0.2),
        },
    ];
    f.handle
        .lessons()
        .record_supplied_lessons(f.requests[0], f.session, &lessons)
        .await?;
    let judge = FakeJudge::new(vec![answer("maybe"), answer("used")]);
    let now = Utc::now() + Duration::hours(1);

    let pass = judge_batch(&f.handle, &judge, now).await;
    assert_eq!(
        (pass.judged, pass.failed, pass.database_failed),
        (1, 1, false)
    );
    let seen = judge
        .seen
        .lock()
        .map_err(|_| anyhow::anyhow!("poisoned"))?
        .clone();
    assert!(seen[0].contains("<<<memory (lesson, mod)>>>"));
    assert!(seen[0].contains("fix the flaky test"));

    let items = f
        .handle
        .lessons()
        .supplied_items_for_request(f.requests[0], 10, 0)
        .await?;
    let judged: Vec<_> = items.iter().filter_map(|i| i.verdict.clone()).collect();
    assert_eq!(judged, vec!["used".to_owned()]);
    let stored: String =
        sqlx::query_scalar("SELECT input_text FROM memory.supplied_item_verdicts LIMIT 1")
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(
        stored, seen[0],
        "the exact input sent is stored with the verdict"
    );

    let second = FakeJudge::new(vec![answer("ignored")]);
    let pass = judge_batch(&f.handle, &second, now).await;
    assert_eq!(
        pass.judged, 1,
        "the item whose answer was refused is judged on the next pass"
    );
    let third = FakeJudge::new(vec![answer("used")]);
    assert_eq!(judge_batch(&f.handle, &third, now).await.judged, 0);
    Ok(())
}

#[tokio::test]
async fn record_standing_lessons_near_turn_attaches_once_and_far_lesson_never() -> anyhow::Result<()>
{
    let f = Fixture::boot().await?;
    let pool = f.handle.pool();
    let near = format!("[1{}]", ",0".repeat(767));
    let far = format!("[0,1{}]", ",0".repeat(766));
    sqlx::query(
        "UPDATE memory.lessons l
            SET embedding = u.vector::vector, embedding_model = 'm', embedded_at = now(),
                created_at = now() - interval '2 days'
           FROM unnest($1::bigint[], $2::text[]) AS u(id, vector)
          WHERE l.id = u.id",
    )
    .bind(&f.lessons[..2])
    .bind(vec![near.clone(), far])
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lesson_routes (lesson_id, project_id, route, revision)
         SELECT id, project_id, 'standing', 1 FROM memory.lessons WHERE id = ANY($1)",
    )
    .bind(&f.lessons[..2])
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.turn_facts
            (operator_turn_id, session_id, builder_version, built_at, max_request_id,
             prompt_kind, prompt_kind_at_close, next_state, next_state_at_close,
             text_chars, text_cut_at, aborted, interrupted, close_cause, ended_on,
             ended_on_version, session_class, session_class_rule)
         SELECT id, session_id, 'test', now(), start_request_id, 'genuine', 'genuine',
                'none_session_settled', 'none_session_settled',
                0, 0, false, false, 'next_prompt', 'none', 'test', 'operator', 'test'
           FROM memory.operator_turns
         ON CONFLICT (operator_turn_id) DO NOTHING",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.semantic_embedders
                (embedder_key, model, dim, chunk_max_chars, prefix_version, urls_seen,
                 canary_version, canary_vectors, status)
         VALUES ('m|768', 'm', 768, 1024, 'doc-prefix-v1', '{}', 'v1', '{}'::jsonb, 'retired')",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.semantic_items
                (kind, session_id, operator_turn_id, project_id, source_ref, text, text_chars,
                 embedding, embedder_key, vector_source, builder_version, effective_at)
         SELECT 'turn_prompt', t.session_id, t.id, s.project_id,
                'memory.operator_turns:' || t.id, 'fix it', 6, $1::text::vector,
                'm|768', 'embedded', 'test', now()
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id",
    )
    .bind(&near)
    .execute(pool)
    .await?;
    let from = Utc::now() - Duration::days(3);
    let before = Utc::now() + Duration::hours(1);

    let first = f
        .handle
        .lessons()
        .record_standing_lessons(from, before)
        .await?;
    let second = f
        .handle
        .lessons()
        .record_standing_lessons(from, before)
        .await?;

    assert_eq!(
        (first, second),
        (1, 0),
        "once for the session, the close lesson only"
    );
    let stored: (i64, String) =
        sqlx::query_as("SELECT lesson_id, route FROM memory.supplied_items")
            .fetch_one(pool)
            .await?;
    assert_eq!(stored, (f.lessons[0], "standing".to_owned()));
    Ok(())
}

#[tokio::test]
async fn judge_batch_waits_for_the_turn_to_settle() -> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    let lessons = [SuppliedLesson {
        lesson_id: f.lessons[0],
        route: "proxy",
        score: None,
    }];
    f.handle
        .lessons()
        .record_supplied_lessons(f.requests[0], f.session, &lessons)
        .await?;
    let judge = FakeJudge::new(vec![answer("used")]);
    let pass = judge_batch(&f.handle, &judge, Utc::now()).await;
    assert_eq!(
        pass.judged, 0,
        "a turn closed under 30 minutes ago is not judged yet"
    );
    Ok(())
}

#[tokio::test]
async fn judge_batch_refused_item_with_decisions_left_judges_the_rest_and_waits_its_turn()
-> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    sqlx::query("ALTER TABLE memory.supplied_items ALTER COLUMN id RESTART WITH 9000001")
        .execute(f.handle.pool())
        .await?;
    let lessons = [
        SuppliedLesson {
            lesson_id: f.lessons[0],
            route: "proxy",
            score: None,
        },
        SuppliedLesson {
            lesson_id: f.lessons[1],
            route: "proxy",
            score: None,
        },
    ];
    for request in [f.requests[0], f.requests[1]] {
        f.handle
            .lessons()
            .record_supplied_lessons(request, f.session, &lessons)
            .await?;
    }
    let refused = Err(JudgeError::from(LlmError::RateLimited { limit: 30 }));
    let judge = FakeJudge::deciding_without_model(vec![
        answer("used"),
        answer("ignored"),
        answer("used"),
        refused,
    ]);
    let now = Utc::now() + Duration::hours(1);

    let pass = judge_batch(&f.handle, &judge, now).await;

    assert_eq!((pass.judged, pass.rate_limited), (3, true));
    let soon = FakeJudge::new(vec![answer("used")]);
    let again = judge_batch(&f.handle, &soon, now + Duration::minutes(10)).await;
    assert_eq!(
        again.judged, 0,
        "the refused item is set aside for two hours"
    );
    let later = FakeJudge::new(vec![answer("used")]);
    let after = judge_batch(&f.handle, &later, now + Duration::hours(3)).await;
    assert_eq!(after.judged, 1, "then it is judged");
    Ok(())
}

#[tokio::test]
async fn judge_batch_stops_at_the_first_rate_limit_and_leaves_the_rest_pending()
-> anyhow::Result<()> {
    let f = Fixture::boot().await?;
    let lessons = [
        SuppliedLesson {
            lesson_id: f.lessons[0],
            route: "proxy",
            score: None,
        },
        SuppliedLesson {
            lesson_id: f.lessons[1],
            route: "proxy",
            score: None,
        },
    ];
    for request in [f.requests[0], f.requests[1]] {
        f.handle
            .lessons()
            .record_supplied_lessons(request, f.session, &lessons)
            .await?;
    }
    let limited = Err(JudgeError::from(LlmError::RateLimited { limit: 30 }));
    let judge = FakeJudge::new(vec![
        answer("used"),
        limited,
        answer("used"),
        answer("ignored"),
    ]);
    let now = Utc::now() + Duration::hours(1);

    let pass = judge_batch(&f.handle, &judge, now).await;

    assert_eq!((pass.judged, pass.failed), (2, 0));
    assert!(pass.rate_limited);
    assert!(!pass.database_failed);
    let calls = judge
        .seen
        .lock()
        .map_err(|_| anyhow::anyhow!("poisoned"))?
        .len();
    assert_eq!(calls, 3, "no call is made after the limiter refused");
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.supplied_item_verdicts")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(
        stored, 2,
        "the refused item and the one after it stay pending"
    );
    Ok(())
}
