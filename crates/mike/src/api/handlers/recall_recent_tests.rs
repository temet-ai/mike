use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

struct Seeded {
    path: String,
    newest: i64,
    middle: i64,
    oldest: i64,
}

async fn add_turn(
    pool: &PgPool,
    (session, project): (Uuid, i64),
    (prompt, day): (&str, u32),
) -> anyhow::Result<i64> {
    seed_session_turns(pool, &session.to_string(), project, 1).await?;
    let turn: i64 = sqlx::query_scalar(
        "UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = 'done',
                created_at = $3
          WHERE id = (SELECT max(t.id) FROM memory.operator_turns t
                        JOIN sessions.sessions s ON s.id = t.session_id
                       WHERE s.session_uuid = $1 AND t.user_prompt IS NULL)
      RETURNING id",
    )
    .bind(session)
    .bind(prompt)
    .bind(Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).single())
    .fetch_one(pool)
    .await?;
    Ok(turn)
}

async fn seed(harness: &Harness) -> anyhow::Result<Seeded> {
    harness
        .state
        .embedding_service()
        .apply_setting("embedding.enabled", &json!(false));
    let pool = harness.handle.pool();
    let path = format!("/recall-recent/{}", Uuid::new_v4().simple());
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
    let owner = (session, project);
    let oldest = add_turn(pool, owner, ("Tune the gibbon cache", 3)).await?;
    let newest = add_turn(pool, owner, ("Size the gibbon cache", 20)).await?;
    let middle = add_turn(pool, owner, ("Warm the gibbon cache", 10)).await?;
    add_turn(pool, owner, ("Order lunch for Friday", 28)).await?;
    Ok(Seeded {
        path,
        newest,
        middle,
        oldest,
    })
}

fn turn_ids_in_order(answer: &Value) -> Vec<i64> {
    answer["turns"]
        .as_array()
        .map(|turns| turns.iter().filter_map(|t| t["turn_id"].as_i64()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn recall_similar_turns_order_recent_returns_matching_turns_newest_first_without_the_reranker()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon", "order": "recent"}),
        )
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(
        turn_ids_in_order(&answer),
        vec![seeded.newest, seeded.middle, seeded.oldest]
    );
    assert_eq!(answer["rerank"]["state"], "off");
    assert!(
        answer["rerank"]["detail"]
            .as_str()
            .is_some_and(|text| text.contains("most recent activity"))
    );
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_order_relevance_still_uses_the_reranker() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon", "order": "relevance"}),
        )
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(answer["rerank"]["state"], "reranked");
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_order_other_than_relevance_or_recent_is_a_400() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": seeded.path, "query": "gibbon", "order": "oldest"}),
        )
        .await?;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("`relevance`") && message.contains("`recent`"),
        "{answer}"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_order_recent_does_not_call_the_reranker() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={}&query=gibbon&order=recent",
            seeded.path
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(answer["rerank"]["state"], "off");
    let found: Vec<String> = answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        found,
        vec![
            format!("turn:{}", seeded.newest),
            format!("turn:{}", seeded.middle),
            format!("turn:{}", seeded.oldest)
        ]
    );
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}
