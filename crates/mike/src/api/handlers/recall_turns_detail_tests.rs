use serde_json::{Value, json};
use uuid::Uuid;

use super::recall_turns_detail::POINTER_PROMPT_CHARS;
use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

const ROUTE: &str = "/api/recall/similar-turns";

struct Seeded {
    project: String,
    session: Uuid,
}

async fn seed_turns(harness: &Harness) -> anyhow::Result<Seeded> {
    let pool = harness.handle.pool();
    let project = format!("/recall-detail/{}", Uuid::new_v4());
    let project_id = seed_project(pool, &project).await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project_id)
    .execute(pool)
    .await?;
    seed_session_turns(pool, &session.to_string(), project_id, 3).await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET user_prompt = 'quicksort question ' || id || ' ' || repeat('w', 300),
                final_assistant_text = 'answer about quicksort ' || id",
    )
    .execute(pool)
    .await?;
    Ok(Seeded { project, session })
}

async fn recall(
    harness: &Harness,
    seeded: &Seeded,
    detail: Option<&str>,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let mut body = json!({"project": seeded.project, "query": "quicksort",
        "session_id": seeded.session, "include_own_turn": true});
    if let Some(detail) = detail {
        body["detail"] = json!(detail);
    }
    harness.post(ROUTE, body).await
}

fn field_names(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[tokio::test]
async fn recall_similar_turns_detail_best_keeps_the_first_turn_whole_and_points_to_the_rest()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;

    let (status, answer) = recall(&harness, &seeded, Some("best")).await?;

    assert!(status.is_success(), "{answer}");
    let turns = answer["turns"].as_array().cloned().unwrap_or_default();
    assert_eq!(turns.len(), 3);
    assert_eq!(
        field_names(&turns[0]),
        vec![
            "created_at",
            "final_assistant_text",
            "outcome",
            "reference",
            "session_id",
            "turn_id",
            "user_prompt"
        ]
    );
    for later in &turns[1..] {
        assert_eq!(
            field_names(later),
            vec!["created_at", "prompt", "reference", "session_id", "turn_id"]
        );
        let id = later["turn_id"].as_i64().unwrap_or_default();
        assert_eq!(later["reference"], format!("turn:{id}"));
        assert_eq!(
            later["prompt"].as_str().map(|p| p.chars().count()),
            Some(POINTER_PROMPT_CHARS)
        );
    }
    assert_eq!(answer["more"]["pointers"], 2);
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_without_detail_or_with_full_returns_every_turn_whole()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;

    let (_, plain) = recall(&harness, &seeded, None).await?;
    let (_, full) = recall(&harness, &seeded, Some("full")).await?;

    assert_eq!(plain, full);
    for turn in plain["turns"].as_array().cloned().unwrap_or_default() {
        assert!(turn.get("user_prompt").is_some(), "{turn}");
    }
    assert!(plain.get("more").is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_with_another_detail_value_is_a_400_naming_both() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed_turns(&harness).await?;

    let (status, answer) = recall(&harness, &seeded, Some("brief")).await?;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("full") && message.contains("best"),
        "{message}"
    );
    harness.cleanup().await
}
