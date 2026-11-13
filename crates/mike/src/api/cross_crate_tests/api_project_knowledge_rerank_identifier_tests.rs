use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

struct Seeded {
    path: String,
    ticket: String,
    plain: Vec<String>,
}

async fn add_turn(pool: &PgPool, session: Uuid, project: i64, prompt: &str) -> anyhow::Result<i64> {
    seed_session_turns(pool, &session.to_string(), project, 1).await?;
    Ok(sqlx::query_scalar(
        "UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = 'done'
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
    let pool = harness.handle.pool();
    let path = format!("/rerank-identifier/{}", Uuid::new_v4().simple());
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
    let mut plain = Vec::new();
    for prompt in [
        "Warm the cache before the release",
        "Explain why the cache goes cold",
        "Size the cache for the nightly build",
    ] {
        plain.push(format!(
            "turn:{}",
            add_turn(pool, session, project, prompt).await?
        ));
    }
    let ticket = add_turn(pool, session, project, "Track XYZ-77 in the tracker").await?;
    Ok(Seeded {
        path,
        ticket: format!("turn:{ticket}"),
        plain,
    })
}

fn items(answer: &Value) -> Vec<Value> {
    answer["items"].as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn knowledge_search_rerank_puts_the_identifier_row_first_though_the_reranker_scored_it_last()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={}&query=cache%20or%20XYZ-77",
            seeded.path
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(answer["rerank"]["state"], "reranked");
    let found = items(&answer);
    assert_eq!(found.len(), 4, "{answer}");
    assert_eq!(found[0]["reference"], seeded.ticket.as_str());
    assert_eq!(found[0]["identifier_hits"], 1);
    let scores: Vec<f64> = found
        .iter()
        .filter_map(|item| item["rerank_score"].as_f64())
        .collect();
    assert!(scores[0] < scores[1], "{scores:?}");
    for item in &found[1..] {
        assert_eq!(item["identifier_hits"], 0);
        assert!(
            seeded
                .plain
                .iter()
                .any(|turn| item["reference"] == turn.as_str())
        );
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_rerank_without_an_identifier_keeps_the_reranker_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness
        .get(&format!(
            "/api/knowledge/search?project={}&query=cache",
            seeded.path
        ))
        .await?;

    assert!(status.is_success(), "{answer}");
    let scores: Vec<f64> = items(&answer)
        .iter()
        .filter_map(|item| item["rerank_score"].as_f64())
        .collect();
    assert_eq!(scores.len(), 3, "{answer}");
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );
    harness.cleanup().await
}
