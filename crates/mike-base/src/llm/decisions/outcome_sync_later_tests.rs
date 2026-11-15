use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::llm::decisions::outcome_sync_support_tests::{Env, NewRecord, pair};
use crate::store::persistence::test_support::seed_session_turns;

struct Other {
    id: i64,
    uuid: Uuid,
}

async fn other_session(env: &Env) -> anyhow::Result<Other> {
    let row: (i64, Uuid) = sqlx::query_as(
        "SELECT id, session_uuid FROM sessions.sessions
          WHERE project_id = $1 AND id <> $2 ORDER BY id LIMIT 1",
    )
    .bind(env.project)
    .bind(env.session_id)
    .fetch_one(env.handle.pool())
    .await?;
    Ok(Other {
        id: row.0,
        uuid: row.1,
    })
}

async fn describe(env: &Env, id: i64, state: Value, answers: Value) -> anyhow::Result<()> {
    sqlx::query("UPDATE decisions.records SET state = $2, answers = $3 WHERE id = $1")
        .bind(id)
        .bind(state)
        .bind(answers)
        .execute(env.handle.pool())
        .await?;
    Ok(())
}

struct Change<'a> {
    kind: &'a str,
    session: i64,
    paths: Vec<String>,
    author: &'a str,
    minutes: i64,
}

async fn change_event(env: &Env, change: Change<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workspace.change_events
             (project_id, checkout, at, kind, session_id, paths, subject, detail)
         VALUES ($1, '/tmp/outcomes-later', $2, $3, $4, $5, 'later', $6)",
    )
    .bind(env.project)
    .bind(Utc::now() + Duration::minutes(change.minutes))
    .bind(change.kind)
    .bind(change.session)
    .bind(change.paths)
    .bind(json!({"author": change.author}))
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn author_record(
    env: &Env,
    file: &str,
    pick: &str,
    sessions: [i64; 2],
) -> anyhow::Result<i64> {
    let id = env
        .record(NewRecord::new("change_author", "chosen"))
        .await?;
    let candidates: Vec<Value> = sessions
        .iter()
        .map(|session| json!({"id": format!("s{session}")}))
        .collect();
    describe(
        env,
        id,
        json!({"changed_file": file, "candidate_sessions": candidates}),
        json!({"who": {"type": "choice", "choice": pick}}),
    )
    .await?;
    Ok(id)
}

#[tokio::test]
async fn change_author_is_scored_against_the_session_that_committed_the_file() -> anyhow::Result<()>
{
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let other = other_session(&env).await?;
    let (a, b) = (env.session_id, other.id);
    let pick_a = format!("s{a}");
    let pick_b = format!("s{b}");
    let unknown = "unknown".to_owned();
    let cases = [
        ("right.rs", &pick_a, a, "shell", 10, Some("right")),
        (
            "wrong.rs",
            &pick_b,
            a,
            "fleet_seat_trailer",
            10,
            Some("wrong"),
        ),
        ("unknown.rs", &unknown, b, "shell", 10, Some("wrong")),
        ("jev.rs", &pick_a, a, "jev", 10, None),
        ("late.rs", &pick_a, a, "shell", 7 * 60, None),
    ];
    let mut ids = Vec::new();
    for (file, pick, committer, author, minutes, _) in cases {
        ids.push(author_record(&env, file, pick, [a, b]).await?);
        let change = Change {
            kind: "commit",
            session: committer,
            paths: vec![file.to_owned()],
            author,
            minutes,
        };
        change_event(&env, change).await?;
    }
    assert_eq!(env.sync().await?, 3);
    for (id, case) in ids.iter().zip(cases) {
        let expected: Vec<_> = case
            .5
            .into_iter()
            .map(|l| pair(l, "next_commit_author"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected, "{}", case.0);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn run_with_fleet(env: &Env, run: &str, state: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(run)
        .execute(env.handle.pool())
        .await?;
    sqlx::query(
        "INSERT INTO launch.fleets (slug, project_path, state, run_id) VALUES ($1, '/tmp/outcomes-later', $2, $1)",
    )
    .bind(run)
    .bind(state)
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn task_event(
    env: &Env,
    (run, kind, session): (&str, &str, Option<Uuid>),
    (payload, minutes): (Value, i64),
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.fleet_events (run_id, task, session_uuid, kind, payload, occurred_at)
         VALUES ($1, 'T1', $2, $3, $4, $5)",
    )
    .bind(run)
    .bind(session)
    .bind(kind)
    .bind(payload)
    .bind(Utc::now() + Duration::minutes(minutes))
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

struct Leaf {
    run: &'static str,
    jev: &'static str,
    fleet: &'static str,
    commit: Option<i64>,
    passed: Option<i64>,
    label: Option<&'static str>,
}

#[tokio::test]
async fn leaf_judge_is_scored_by_what_the_task_did_after_the_verdict() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let cases = [
        Leaf {
            run: "pass-quiet",
            jev: "pass",
            fleet: "done",
            commit: None,
            passed: None,
            label: Some("right"),
        },
        Leaf {
            run: "pass-commit",
            jev: "pass",
            fleet: "failed",
            commit: Some(5),
            passed: None,
            label: Some("wrong"),
        },
        Leaf {
            run: "pass-open",
            jev: "pass",
            fleet: "running",
            commit: None,
            passed: None,
            label: None,
        },
        Leaf {
            run: "fail-fixed",
            jev: "fail",
            fleet: "done",
            commit: Some(5),
            passed: Some(10),
            label: Some("right"),
        },
        Leaf {
            run: "fail-unchanged",
            jev: "fail",
            fleet: "done",
            commit: None,
            passed: Some(10),
            label: Some("wrong"),
        },
        Leaf {
            run: "fail-open",
            jev: "fail",
            fleet: "done",
            commit: Some(5),
            passed: None,
            label: None,
        },
    ];
    let mut ids = Vec::new();
    for case in &cases {
        let note = Uuid::new_v4();
        run_with_fleet(&env, case.run, case.fleet).await?;
        task_event(
            &env,
            (case.run, "report_filed", Some(env.session_uuid)),
            (json!({"note_uuid": note}), -1),
        )
        .await?;
        let mut record = NewRecord::new("leaf_judge", "chosen")
            .verdict(json!({"score": 0.9, "verdict": case.jev}));
        record.message_uuid = Some(note);
        ids.push(env.record(record).await?);
        if let Some(minutes) = case.commit {
            task_event(
                &env,
                (case.run, "commit_landed", Some(env.session_uuid)),
                (json!({"sha": "abc"}), minutes),
            )
            .await?;
        }
        if let Some(minutes) = case.passed {
            let verdict = json!({"note_uuid": Uuid::new_v4(), "criterion": "C1", "passed": true});
            task_event(
                &env,
                (case.run, "verdict_recorded", None),
                (verdict, minutes),
            )
            .await?;
        }
    }
    assert_eq!(env.sync().await?, 4);
    for (id, case) in ids.iter().zip(&cases) {
        let expected: Vec<_> = case
            .label
            .into_iter()
            .map(|l| pair(l, "task_after_pass"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected, "{}", case.run);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn thread(
    env: &Env,
    slug: &str,
    status: &str,
    entry_hours: Option<i64>,
) -> anyhow::Result<()> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO work.threads (project_id, slug, title, status) VALUES ($1, $2, $2, $3)
         RETURNING id",
    )
    .bind(env.project)
    .bind(slug)
    .bind(status)
    .fetch_one(env.handle.pool())
    .await?;
    if let Some(hours) = entry_hours {
        sqlx::query(
            "INSERT INTO work.thread_log (thread_id, ord, summary, kind, created_at)
             VALUES ($1, 1, 'worked on', 'checkpoint', now() + ($2::int * interval '1 hour'))",
        )
        .bind(id)
        .bind(i32::try_from(hours)?)
        .execute(env.handle.pool())
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_thread_status_suggestion_is_wrong_when_the_thread_got_a_checkpoint_within_a_day()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let cases = [
        ("moving", "active", Some(1), Some("wrong")),
        ("quiet", "active", None, None),
        ("late", "active", Some(25), None),
        ("earlier", "active", Some(-1), None),
        ("closed", "done", Some(1), None),
    ];
    let mut ids = Vec::new();
    for (slug, status, hours, _) in cases {
        thread(&env, slug, status, hours).await?;
        let mut record = NewRecord::new("thread_status", "suggested")
            .verdict(json!({"score": 0.9, "slug": slug}));
        record.project_id = Some(env.project);
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 1);
    for (id, (_, _, _, label)) in ids.iter().zip(cases) {
        let expected: Vec<_> = label
            .into_iter()
            .map(|l| pair(l, "thread_kept_moving"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn session_judge_is_scored_by_whether_the_settled_session_shipped() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let other = other_session(&env).await?;
    seed_session_turns(env.handle.pool(), &other.uuid.to_string(), env.project, 1).await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET outcome_source = 'frustration_close',
                outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'abandoned')
          WHERE session_id = $1",
    )
    .bind(other.id)
    .execute(env.handle.pool())
    .await?;
    change_event(
        &env,
        Change {
            kind: "push",
            session: env.session_id,
            paths: Vec::new(),
            author: "shell",
            minutes: -30,
        },
    )
    .await?;
    let cases = [
        (env.session_id, "success", Some("right")),
        (env.session_id, "failure", Some("wrong")),
        (other.id, "failure", Some("right")),
        (other.id, "success", Some("wrong")),
        (env.session_id, "partial", None),
    ];
    let mut ids = Vec::new();
    for (session, outcome, _) in cases {
        let mut record = NewRecord::new("session_judge", "sent")
            .verdict(json!({"score": 0.9, "outcome": outcome}));
        record.session_id = Some(session);
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 0);
    sqlx::query(
        "UPDATE sessions.sessions SET last_seen = now() - interval '3 hours' WHERE project_id = $1",
    )
    .bind(env.project)
    .execute(env.handle.pool())
    .await?;
    assert_eq!(env.sync().await?, 4);
    for (id, (_, _, label)) in ids.iter().zip(cases) {
        let expected: Vec<_> = label
            .into_iter()
            .map(|l| pair(l, "session_shipped"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
