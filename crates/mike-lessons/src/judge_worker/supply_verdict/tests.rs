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
}

impl FakeJudge {
    fn new(answers: Vec<Result<Answer, JudgeError>>) -> Self {
        Self {
            answers: Mutex::new(answers),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl SupplyJudge for FakeJudge {
    fn model(&self) -> &str {
        "fake-judge"
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
