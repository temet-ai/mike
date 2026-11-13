use mike::store::persistence::knowledge_repo::KnowledgePersist as _;
use mike::store::test_support::seed_project;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

const FINAL_TEXT: &str = "Built the reader. I'll open the PR once CI is green.";
const PROMISE: &str = "I'll open the PR once CI is green.";

async fn seed_promise(f: &Fixture) -> anyhow::Result<(i64, i64)> {
    insert_request(f, &Req::primary(9001, 0, "add the promise reader")).await?;
    let turn = insert_turn(
        f,
        &NewTurn {
            final_text: Some(FINAL_TEXT),
            ..NewTurn::closed(9001, 9001, 0, 30)
        },
    )
    .await?;
    let start = FINAL_TEXT.find("I'll").unwrap_or(0);
    let promise: i64 = sqlx::query_scalar(
        "INSERT INTO memory.dossier_entries (session_id, operator_turn_id, kind, data, source, source_ref,
                                             dedup_key, extractor_version, effective_at)
         VALUES ($1, $2, 'promise', jsonb_build_object('span', jsonb_build_array($3::int, $4::int)),
                 'capture', 'requests.requests:9001', 'turn:' || $2::text, 'promises-v1', now())
         RETURNING id",
    )
    .bind(f.session_id)
    .bind(turn)
    .bind(i32::try_from(start)?)
    .bind(i32::try_from(FINAL_TEXT.chars().count())?)
    .fetch_one(f.handle.pool())
    .await?;
    Ok((turn, promise))
}

#[tokio::test]
async fn read_project_knowledge_promise_reference_returns_text_session_and_turn()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (turn, promise) = seed_promise(&f).await?;
    let session_uuid: uuid::Uuid =
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?;

    let read = f
        .handle
        .knowledge()
        .read_project_knowledge(Some("turn-facts"), &format!("promise:{promise}"))
        .await?;

    assert_eq!(read["reference"], format!("promise:{promise}"));
    assert_eq!(read["promise_text"], PROMISE);
    assert_eq!(read["body"], FINAL_TEXT);
    assert_eq!(read["status"], "open");
    assert_eq!(read["session_uuid"], session_uuid.to_string());
    assert_eq!(read["turn"], format!("turn:{turn}"));
    assert_eq!(read["user_prompt"], "prompt");
    assert!(read["effective_at"].is_string());
    assert!(read["project_uuid"].is_string());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn read_project_knowledge_promise_from_other_project_is_not_found() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (_, promise) = seed_promise(&f).await?;
    seed_project(f.handle.pool(), "promise-elsewhere").await?;

    let elsewhere = f
        .handle
        .knowledge()
        .read_project_knowledge(Some("promise-elsewhere"), &format!("promise:{promise}"))
        .await;
    let missing = f
        .handle
        .knowledge()
        .read_project_knowledge(Some("turn-facts"), &format!("promise:{}", promise + 1))
        .await;

    assert!(elsewhere.is_err());
    assert!(missing.is_err());
    f.db.cleanup().await?;
    Ok(())
}
