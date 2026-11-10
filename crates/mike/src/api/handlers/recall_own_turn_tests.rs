use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

struct Seeded {
    path: String,
    session: Uuid,
    older: i64,
    newest: i64,
}

async fn add_turn(pool: &PgPool, session: Uuid, project: i64, prompt: &str) -> anyhow::Result<i64> {
    seed_session_turns(pool, &session.to_string(), project, 1).await?;
    Ok(sqlx::query_scalar(
        "UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = ''
          WHERE id = (SELECT max(t.id) FROM memory.operator_turns t
                        JOIN sessions.sessions s ON s.id = t.session_id
                       WHERE s.session_uuid = $1 AND t.user_prompt IS NULL)
      RETURNING id",
    )
    .bind(session)
    .bind(prompt)
    .fetch_one(pool)
    .await?)
}

async fn seed(harness: &Harness) -> anyhow::Result<Seeded> {
    harness
        .state
        .embedding_service()
        .apply_setting("embedding.enabled", &json!(false));
    let pool = harness.handle.pool();
    let path = format!("/recall-own/{}", Uuid::new_v4().simple());
    let project = seed_project(pool, &path).await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(session)
    .bind(project)
    .execute(pool)
    .await?;
    let older = add_turn(pool, session, project, "Tune the gibbon cache").await?;
    let newest = add_turn(pool, session, project, "Gibbon cache tuning again please").await?;
    Ok(Seeded {
        path,
        session,
        older,
        newest,
    })
}

fn turn_ids(answer: &Value) -> Vec<i64> {
    let mut ids: Vec<i64> = answer["turns"]
        .as_array()
        .map(|turns| turns.iter().filter_map(|t| t["turn_id"].as_i64()).collect())
        .unwrap_or_default();
    ids.sort_unstable();
    ids
}

fn item_turns(answer: &Value) -> Vec<String> {
    let mut found: Vec<String> = answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn both_turns(seeded: &Seeded) -> Vec<i64> {
    vec![seeded.older, seeded.newest]
}

fn both_references(seeded: &Seeded) -> Vec<String> {
    let mut found = vec![
        format!("turn:{}", seeded.older),
        format!("turn:{}", seeded.newest),
    ];
    found.sort();
    found
}

#[tokio::test]
async fn recall_similar_turns_with_a_session_id_leaves_out_that_sessions_newest_turn()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon", "session_id": seeded.session}),
        )
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(turn_ids(&answer), vec![seeded.older]);
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_include_own_turn_returns_the_newest_turn() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon", "session_id": seeded.session,
                   "include_own_turn": true}),
        )
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(turn_ids(&answer), both_turns(&seeded));
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_without_a_session_id_returns_every_turn() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon"}),
        )
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(turn_ids(&answer), both_turns(&seeded));
    harness.cleanup().await
}

#[tokio::test]
async fn recall_project_context_with_a_session_id_leaves_out_that_sessions_newest_turn()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .get(&format!(
            "/api/recall/project-context?project={}&query=gibbon&session_id={}",
            seeded.path, seeded.session
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(
        item_turns(&answer["knowledge"]),
        vec![format!("turn:{}", seeded.older)]
    );
    harness.cleanup().await
}

#[tokio::test]
async fn recall_project_context_with_include_own_turn_returns_the_newest_turn() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .get(&format!(
            "/api/recall/project-context?project={}&query=gibbon&session_id={}&include_own_turn=true",
            seeded.path, seeded.session
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(item_turns(&answer["knowledge"]), both_references(&seeded));
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_with_a_session_id_leaves_out_that_sessions_newest_turn_unless_asked()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    let route = format!(
        "/api/knowledge/search?project={}&query=gibbon&session_id={}",
        seeded.path, seeded.session
    );

    let (status, hidden) = harness.get(&route).await?;
    let (_, shown) = harness
        .get(&format!("{route}&include_own_turn=true"))
        .await?;

    assert!(status.is_success(), "{hidden}");
    assert_eq!(item_turns(&hidden), vec![format!("turn:{}", seeded.older)]);
    assert_eq!(item_turns(&shown), both_references(&seeded));
    harness.cleanup().await
}
