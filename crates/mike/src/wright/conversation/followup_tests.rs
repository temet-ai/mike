use std::sync::Arc;

use serde_json::{Value, json};
use uuid::Uuid;

use super::fakes_tests::{Db, FakeEngine, FakeSteers};
use super::job::Processed;
use super::worker::tick;

const CHECK: &str = "undelivered_mail";
const ISSUE_URL: &str = concat!("https:", "/", "/example.test/issues/694");

async fn problem(db: &Db) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO wright.problems (check_name, source, scope, resource, state)
         VALUES ($1, 'capture_db', 'mail', $2, 'open') RETURNING id",
    )
    .bind(CHECK)
    .bind(Uuid::new_v4().to_string())
    .fetch_one(db.handle.pool())
    .await?)
}

async fn handed_off_followup(db: &Db) -> anyhow::Result<(i64, Uuid)> {
    let session = Uuid::new_v4();
    let id = sqlx::query_scalar(
        "INSERT INTO wright.followups (kind, check_name, title, body, state, issue_number, issue_url,
                                       filed_at, handed_off_at, fixer_session, fixer_machine)
         VALUES ('root_cause', $1, 'why mail waits', 'body', 'handed_off', 694,
                 $3, now(), now(), $2, 'omit')
         RETURNING id",
    )
    .bind(CHECK)
    .bind(session)
    .bind(ISSUE_URL)
    .fetch_one(db.handle.pool())
    .await?;
    Ok((id, session))
}

#[tokio::test]
async fn answer_about_a_problem_sees_its_followups_and_keeps_only_followup_links_that_exist()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let problem = problem(&db).await?;
    let (followup, session) = handed_off_followup(&db).await?;
    let missing = followup + 1_000_000;
    let message = db
        .ask(
            "ask-followup",
            &format!("what is filed for problem {problem}?"),
        )
        .await?;
    let engine = FakeEngine::answering(vec![Ok(json!({
        "body": "filed as #694; a session on omit is fixing it",
        "links": [
            {"kind": "problem", "id": problem},
            {"kind": "followup", "id": followup},
            {"kind": "followup", "id": missing},
        ],
    }))]);
    let convo = db.convo(engine.clone(), Arc::new(FakeSteers::default()));
    let outcome = tick(&convo).await?;
    assert!(
        matches!(outcome.processed, Processed::Answered(_)),
        "{outcome:?}"
    );

    let requests = engine
        .requests
        .lock()
        .map(|r| r.clone())
        .unwrap_or_default();
    let request = requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("the engine saw no request"))?;
    assert!(request.brief.contains("follow-up"), "{}", request.brief);
    assert!(request.brief.contains("followup"), "{}", request.brief);
    let (_, evidence) = request
        .inputs
        .iter()
        .find(|(name, _)| name == "evidence.json")
        .ok_or_else(|| anyhow::anyhow!("no evidence.json input"))?;
    let listed = evidence["problems"]
        .as_array()
        .and_then(|problems| problems.iter().find(|p| p["problem_id"] == problem))
        .ok_or_else(|| anyhow::anyhow!("problem {problem} is not in the evidence: {evidence}"))?;
    let item = &listed["followups"][0];
    assert_eq!(item["followup_id"], followup);
    assert_eq!(item["kind"], "root_cause");
    assert_eq!(item["state"], "handed_off");
    assert_eq!(item["issue_number"], 694);
    assert_eq!(item["issue_url"], ISSUE_URL);
    assert_eq!(item["fixer_session"], session.to_string());
    assert_eq!(item["fixer_machine"], "omit");

    let links: Value = sqlx::query_scalar(
        "SELECT links FROM wright.messages WHERE author = 'wright' AND reply_to = $1",
    )
    .bind(message)
    .fetch_one(db.handle.pool())
    .await?;
    assert_eq!(
        links,
        json!([
            {"kind": "problem", "id": problem},
            {"kind": "followup", "id": followup},
        ])
    );
    db.cleanup().await
}
