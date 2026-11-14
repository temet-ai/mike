use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};

struct Other {
    id: i64,
    uuid: Uuid,
}

async fn other_session(env: &Env) -> anyhow::Result<Other> {
    let row = sqlx::query!(
        "SELECT id, session_uuid FROM sessions.sessions
          WHERE project_id = $1 AND id <> $2 ORDER BY id LIMIT 1",
        env.project,
        env.session_id
    )
    .fetch_one(env.handle.pool())
    .await?;
    Ok(Other {
        id: row.id,
        uuid: row.session_uuid,
    })
}

async fn run(env: &Env, name: &str) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')",
        name
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn seat(env: &Env, run: &str, seat: &str, session: Uuid) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO launch.launches (backend, folder, run_id, session_uuid, seat)
         VALUES ('herdr', '/tmp/outcomes', $1, $2, $3)",
        run,
        session,
        seat
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn event(
    env: &Env,
    (run, kind, session): (&str, &str, Option<Uuid>),
    (payload, minutes_from_now): (Value, i64),
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO launch.fleet_events (run_id, task, session_uuid, kind, payload, occurred_at)
         VALUES ($1, 'T1', $2, $3, $4, $5)",
        run,
        session,
        kind,
        payload,
        Utc::now() + Duration::minutes(minutes_from_now),
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn gate(env: &Env, run: &str, sha: &str, passed: bool, minutes: i64) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO workspace.change_events
             (project_id, checkout, at, kind, run_id, paths, subject, detail, after_sha)
         VALUES ($1, '/tmp/outcomes', $2, 'check', $3, '{}', 'gate', $4, $5)",
        env.project,
        Utc::now() + Duration::minutes(minutes),
        run,
        json!({"label": "gate", "passed": passed}),
        sha,
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn gate_blame_is_right_when_the_blamed_seat_committed_before_the_next_pass()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let other = other_session(&env).await?;
    run(&env, "run-gate").await?;
    seat(&env, "run-gate", "worker-A", env.session_uuid).await?;
    seat(&env, "run-gate", "worker-B", other.uuid).await?;
    gate(&env, "run-gate", "failedsha", false, -30).await?;
    gate(&env, "run-gate", "fixedsha", true, 30).await?;
    event(
        &env,
        ("run-gate", "commit_landed", Some(env.session_uuid)),
        (json!({"sha": "fixedsha"}), 10),
    )
    .await?;
    let mut ids = Vec::new();
    for picked in [Some("worker-A"), Some("worker-B"), None] {
        let mut record = NewRecord::new("gate_blame", "chosen")
            .verdict(json!({"score": 0.8, "picked_seat": picked}));
        record.commit_sha = Some("failedsha".to_owned());
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(ids[0]).await?,
        vec![pair("right", "next_gate_pass")]
    );
    assert_eq!(
        env.labels(ids[1]).await?,
        vec![pair("wrong", "next_gate_pass")]
    );
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn problem_solver_is_wrong_when_the_problem_came_back_within_a_day() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    run(&env, "run-problem").await?;
    let cases = [
        (
            "reopened",
            "instruct_seats",
            2,
            Some("problem_opened"),
            "wrong",
        ),
        (
            "escalated",
            "record_fault",
            2,
            Some("problem_escalated"),
            "wrong",
        ),
        (
            "own-escalation",
            "escalate",
            2,
            Some("problem_escalated"),
            "right",
        ),
        ("quiet", "new_task", 2, None, "right"),
    ];
    let mut ids = Vec::new();
    for (name, decision, age, follow_up, _) in cases {
        let problem = Uuid::new_v4().to_string();
        let mut record = NewRecord::new("problem_solver", "chosen").verdict(
            json!({"score": 0.8, "problem_id": problem, "decision": decision, "name": name}),
        );
        record.age_days = age;
        ids.push(env.record(record).await?);
        if let Some(kind) = follow_up {
            event(
                &env,
                ("run-problem", kind, None),
                (json!({"problem_id": problem}), -47 * 60),
            )
            .await?;
        }
    }
    let mut fresh = NewRecord::new("problem_solver", "chosen").verdict(
        json!({"score": 0.8, "problem_id": Uuid::new_v4().to_string(), "decision": "new_task"}),
    );
    fresh.age_days = 0;
    let fresh = env.record(fresh).await?;
    assert_eq!(env.sync().await?, 4);
    for (id, (_, _, _, _, label)) in ids.iter().zip(cases) {
        assert_eq!(
            env.labels(*id).await?,
            vec![pair(label, "problem_after_24h")]
        );
    }
    assert!(env.labels(fresh).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn leaf_judge_is_scored_against_the_acceptance_seats_verdict_on_the_same_report()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    run(&env, "run-leaf").await?;
    let cases = [
        ("pass", Some(true), Some("right")),
        ("pass", Some(false), Some("wrong")),
        ("fail", Some(false), Some("right")),
        ("fail", Some(true), Some("wrong")),
        ("pass", None, None),
    ];
    let mut ids = Vec::new();
    for (jev, acceptance, _) in cases {
        let note = Uuid::new_v4();
        let mut record =
            NewRecord::new("leaf_judge", "chosen").verdict(json!({"score": 0.9, "verdict": jev}));
        record.message_uuid = Some(note);
        ids.push(env.record(record).await?);
        let verdict =
            |passed: bool| json!({"note_uuid": note, "criterion": "C1", "passed": passed});
        event(
            &env,
            ("run-leaf", "verdict_recorded", None),
            (verdict(true), 0),
        )
        .await?;
        if let Some(passed) = acceptance {
            event(
                &env,
                ("run-leaf", "verdict_recorded", Some(env.session_uuid)),
                (verdict(passed), 1),
            )
            .await?;
        }
    }
    assert_eq!(env.sync().await?, 4);
    for (id, (_, _, label)) in ids.iter().zip(cases) {
        let expected: Vec<_> = label
            .into_iter()
            .map(|l| pair(l, "acceptance_verdict"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[derive(Clone, Copy)]
struct Mail {
    consumed_min: Option<i64>,
    sent_request: Option<i64>,
    replied: bool,
    queued_min_ago: i64,
}

async fn reply_to(
    env: &Env,
    other: &Other,
    id: i64,
    at: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO mailbox.messages
             (message_uuid, to_session_id, from_session_id, body, queued_at, in_reply_to_id)
         VALUES ($1, $2, $3, 'reply', $4, $5)",
        Uuid::new_v4(),
        other.id,
        env.session_id,
        at,
        id,
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn woken_mail(env: &Env, other: &Other, mail: &Mail) -> anyhow::Result<i64> {
    let message_uuid = Uuid::new_v4();
    let queued = Utc::now() - Duration::minutes(mail.queued_min_ago);
    let id = sqlx::query_scalar!(
        "INSERT INTO mailbox.messages
             (message_uuid, to_session_id, from_session_id, body, queued_at, consumed_at,
              sent_request_id, sent_at)
         VALUES ($1, $2, $3, 'hello', $4, $5, $6, $7)
         RETURNING id",
        message_uuid,
        env.session_id,
        other.id,
        queued,
        mail.consumed_min.map(|m| queued + Duration::minutes(m)),
        mail.sent_request,
        mail.sent_request.map(|_| queued + Duration::minutes(1)),
    )
    .fetch_one(env.handle.pool())
    .await?;
    if mail.replied {
        reply_to(env, other, id, queued + Duration::minutes(4)).await?;
    }
    let mut record = NewRecord::new("fleet_wake_check", "sent");
    record.message_uuid = Some(message_uuid);
    env.record(record).await
}

#[tokio::test]
async fn a_woken_message_is_right_when_the_seat_consumed_and_acted_within_ten_minutes()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let other = other_session(&env).await?;
    let request = env.turns().await?[0].start_request_id;
    let base = Mail {
        consumed_min: Some(3),
        sent_request: None,
        replied: false,
        queued_min_ago: 60,
    };
    let replied = woken_mail(
        &env,
        &other,
        &Mail {
            replied: true,
            ..base
        },
    )
    .await?;
    let carried = woken_mail(
        &env,
        &other,
        &Mail {
            sent_request: Some(request),
            ..base
        },
    )
    .await?;
    let ignored = woken_mail(
        &env,
        &other,
        &Mail {
            consumed_min: None,
            ..base
        },
    )
    .await?;
    let slow = woken_mail(
        &env,
        &other,
        &Mail {
            consumed_min: Some(30),
            replied: true,
            ..base
        },
    )
    .await?;
    let open = woken_mail(
        &env,
        &other,
        &Mail {
            queued_min_ago: 2,
            replied: true,
            ..base
        },
    )
    .await?;
    assert_eq!(env.sync().await?, 4);
    let source = "woken_seat_acted";
    assert_eq!(env.labels(replied).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(carried).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(ignored).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(slow).await?, vec![pair("wrong", source)]);
    assert!(env.labels(open).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn set_operator_turns(env: &Env) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE memory.operator_turns SET opened_by = 'operator' WHERE session_id = $1",
        env.session_id
    )
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn probe_answers_are_checked_against_the_operator_turns_the_session_later_had()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let other = other_session(&env).await?;
    set_operator_turns(&env).await?;
    let cases = [
        (env.session_id, true, 2, Some("wrong")),
        (other.id, true, 2, Some("right")),
        (env.session_id, false, 2, Some("right")),
        (other.id, false, 2, None),
        (other.id, true, 0, None),
    ];
    let mut ids = Vec::new();
    for (session, is_probe, age, _) in cases {
        let mut record = NewRecord::new("probe_session", "chosen").verdict(json!({
            "score": 0.8,
            "sessions": [{"session_id": session, "is_probe": is_probe}],
        }));
        record.age_days = age;
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 3);
    for (id, (_, _, _, label)) in ids.iter().zip(cases) {
        let expected: Vec<_> = label
            .into_iter()
            .map(|l| pair(l, "later_operator_turns"))
            .collect();
        assert_eq!(env.labels(*id).await?, expected);
    }
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn recall_topic_is_scored_by_the_verdicts_on_the_lessons_it_added() -> anyhow::Result<()> {
    let Some(env) = Env::boot(2).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let mut ids = Vec::new();
    for (turn, topic, said) in [
        (&turns[0], "deploys", "used"),
        (&turns[1], "schemas", "ignored"),
    ] {
        let lesson = env.lesson().await?;
        sqlx::query!(
            "UPDATE memory.lessons SET topic = $2 WHERE id = $1",
            lesson,
            topic
        )
        .execute(env.handle.pool())
        .await?;
        let item = sqlx::query_scalar!(
            "INSERT INTO memory.supplied_items
                 (session_id, request_id, item_kind, lesson_id, route, item_text)
             VALUES ($1, $2, 'lesson', $3, 'proxy', 'text') RETURNING id",
            env.session_id,
            turn.start_request_id,
            lesson
        )
        .fetch_one(env.handle.pool())
        .await?;
        sqlx::query!(
            "INSERT INTO memory.supplied_item_verdicts
                 (supplied_item_id, verdict, rationale, model, input_text)
             VALUES ($1, $2, 'r', 'm', 'i')",
            item,
            said
        )
        .execute(env.handle.pool())
        .await?;
        let mut record = NewRecord::new("recall_topic", "chosen")
            .verdict(json!({"score": 0.8, "mapped_topics": [topic]}));
        record.request_id = Some(turn.start_request_id);
        record.project_id = Some(env.project);
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    let source = "recalled_lesson_verdict";
    assert_eq!(env.labels(ids[0]).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(ids[1]).await?, vec![pair("wrong", source)]);
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
