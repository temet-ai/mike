use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

struct Seeded {
    name: String,
    needle: String,
    crowded: Uuid,
    newest: Uuid,
}

async fn session(harness: &Harness, project_id: i64, turns: i64) -> anyhow::Result<Uuid> {
    let pool = harness.handle.pool();
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project_id)
    .execute(pool)
    .await?;
    seed_session_turns(pool, &session.to_string(), project_id, turns).await?;
    Ok(session)
}

async fn chunk(harness: &Harness, session: Uuid, role: &str, text: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT t.id, 0, cr.id, $3,
                ('[' || array_to_string(array_fill(0.1::float4, ARRAY[768]), ',') || ']')::vector,
                'test-model'
           FROM sessions.sessions s
           JOIN memory.operator_turns t ON t.session_id = s.id
           JOIN memory.chunk_roles cr ON cr.code = $2
          WHERE s.session_uuid = $1
            AND NOT EXISTS (SELECT 1 FROM memory.operator_turn_chunks c WHERE c.turn_id = t.id)
          ORDER BY t.id
          LIMIT 1",
    )
    .bind(session)
    .bind(role)
    .bind(text)
    .execute(harness.handle.pool())
    .await?;
    Ok(())
}

async fn seed(harness: &Harness) -> anyhow::Result<Seeded> {
    let unique = Uuid::new_v4().simple().to_string();
    let name = format!("cs-name-{unique}");
    let needle = format!("needle{unique}");
    let project_id = seed_project(harness.handle.pool(), &format!("/cs/{unique}")).await?;
    sqlx::query("UPDATE projects.projects SET custom_name = $1 WHERE id = $2")
        .bind(&name)
        .bind(project_id)
        .execute(harness.handle.pool())
        .await?;
    let crowded = session(harness, project_id, 3).await?;
    for _ in 0..3 {
        chunk(harness, crowded, "prompt", &format!("the {needle} again")).await?;
    }
    let newest = session(harness, project_id, 1).await?;
    chunk(
        harness,
        newest,
        "assistant",
        &format!("one {needle} mention"),
    )
    .await?;
    Ok(Seeded {
        name,
        needle,
        crowded,
        newest,
    })
}

fn session_ids(answer: &Value) -> Vec<String> {
    answer["sessions"]
        .as_array()
        .map(|sessions| {
            sessions
                .iter()
                .filter_map(|s| s["session_id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn search(harness: &Harness, query: &str) -> anyhow::Result<(StatusCode, Value)> {
    harness.get(&format!("/api/content/search?{query}")).await
}

#[tokio::test]
async fn content_search_with_a_caller_logs_one_row_with_the_sessions_and_match_counts()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(
        &harness,
        &format!("q={}&caller_session_id={}", seeded.needle, seeded.newest),
    )
    .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    let events = harness.events().await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tool, "search_session_content");
    assert_eq!(events[0].query, seeded.needle);
    assert_eq!(events[0].rerank_state, "off");
    assert!(events[0].session_id.is_some());
    assert_eq!(
        events[0].returned,
        serde_json::json!([
            {"reference": format!("session:{}", seeded.crowded), "matches": 3},
            {"reference": format!("session:{}", seeded.newest), "matches": 1}
        ])
    );
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_without_a_caller_logs_no_row() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, _) = search(&harness, &format!("q={}", seeded.needle)).await?;

    assert_eq!(status, StatusCode::OK);
    assert!(harness.events().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_project_given_by_name_returns_that_projects_matches() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(
        &harness,
        &format!("q={}&project={}", seeded.needle, seeded.name),
    )
    .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["total_sessions"], 2, "{answer}");
    assert_eq!(answer["total_matches"], 4, "{answer}");
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_unknown_project_is_an_error_not_a_zero() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(
        &harness,
        &format!("q={}&project=no-such-project-anywhere", seeded.needle),
    )
    .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("Unknown project `no-such-project-anywhere`"),
        "{message}"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_default_order_puts_the_session_with_more_matches_above_a_newer_one()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(&harness, &format!("q={}", seeded.needle)).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["order"], "matches");
    assert_eq!(
        session_ids(&answer),
        vec![seeded.crowded.to_string(), seeded.newest.to_string()]
    );
    assert_eq!(answer["sessions"][0]["prompt_match_count"], 3);
    assert_eq!(answer["sessions"][1]["prompt_match_count"], 0);
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_order_recent_puts_the_newest_session_first() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(&harness, &format!("q={}&order=recent", seeded.needle)).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["order"], "recent");
    assert_eq!(
        session_ids(&answer),
        vec![seeded.newest.to_string(), seeded.crowded.to_string()]
    );
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_unknown_order_is_a_400_naming_both_values() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = search(&harness, "q=anything&order=newest").await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("`matches`") && message.contains("`recent`"),
        "{message}"
    );
    assert!(message.contains("`newest`"), "{message}");
    harness.cleanup().await
}

struct Words {
    first: String,
    second: String,
    apart: Uuid,
    lone_busy: Uuid,
    together: Uuid,
}

async fn seed_words(harness: &Harness) -> anyhow::Result<Words> {
    let unique = Uuid::new_v4().simple().to_string();
    let first = format!("alpha{unique}");
    let second = format!("omega{unique}");
    let project_id = seed_project(harness.handle.pool(), &format!("/csw/{unique}")).await?;
    let apart = session(harness, project_id, 2).await?;
    chunk(harness, apart, "prompt", &format!("first {first} here")).await?;
    chunk(harness, apart, "prompt", &format!("then {second} there")).await?;
    let lone_busy = session(harness, project_id, 3).await?;
    for _ in 0..3 {
        chunk(harness, lone_busy, "prompt", &format!("only {first} again")).await?;
    }
    let together = session(harness, project_id, 1).await?;
    chunk(harness, together, "prompt", &format!("{first} {second}")).await?;
    Ok(Words {
        first,
        second,
        apart,
        lone_busy,
        together,
    })
}

#[tokio::test]
async fn content_search_all_mode_matches_a_session_whose_words_are_in_different_chunks()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let words = seed_words(&harness).await?;

    let (status, answer) = search(
        &harness,
        &format!("q={}+{}&mode=all", words.first, words.second),
    )
    .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["mode"], "all");
    assert_eq!(answer["total_sessions"], 2, "{answer}");
    assert_eq!(
        session_ids(&answer),
        vec![words.apart.to_string(), words.together.to_string()]
    );
    assert_eq!(answer["sessions"][0]["words_matched"], 2);
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_any_mode_ranks_by_distinct_words_before_match_count() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let words = seed_words(&harness).await?;

    let (status, answer) = search(
        &harness,
        &format!("q={}+{}&mode=any", words.first, words.second),
    )
    .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["total_sessions"], 3, "{answer}");
    assert_eq!(
        session_ids(&answer),
        vec![
            words.apart.to_string(),
            words.together.to_string(),
            words.lone_busy.to_string()
        ]
    );
    assert_eq!(answer["sessions"][2]["words_matched"], 1);
    assert_eq!(answer["sessions"][2]["match_count"], 3);
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_without_a_mode_still_matches_the_literal_phrase() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let words = seed_words(&harness).await?;
    let query = format!("q={}+{}", words.first, words.second);

    let (status, default_answer) = search(&harness, &query).await?;
    let (_, phrase_answer) = search(&harness, &format!("{query}&mode=phrase")).await?;

    assert_eq!(status, StatusCode::OK, "{default_answer}");
    assert_eq!(default_answer["mode"], "phrase");
    assert_eq!(
        session_ids(&default_answer),
        vec![words.together.to_string()]
    );
    assert_eq!(session_ids(&phrase_answer), session_ids(&default_answer));
    assert!(default_answer["sessions"][0].get("words_matched").is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_unknown_mode_is_a_400_naming_the_value() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = search(&harness, "q=anything&mode=fuzzy").await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(message.contains("`fuzzy`"), "{message}");
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_word_mode_over_the_word_cap_is_a_400() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = search(&harness, "q=a+b+c+d+e+f+g+h+i&mode=any").await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("at most 8") && message.contains("got 9"),
        "{message}"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_all_mode_matches_words_too_short_for_the_index() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let words = seed_words(&harness).await?;

    let (status, answer) = search(&harness, &format!("q={}+he&mode=all", words.first)).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(session_ids(&answer), vec![words.apart.to_string()]);
    assert_eq!(answer["total_matches"], 2, "{answer}");
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_shows_fifty_matches_of_a_session_and_counts_the_rest() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    let unique = Uuid::new_v4().simple().to_string();
    let needle = format!("needle{unique}");
    let project_id = seed_project(harness.handle.pool(), &format!("/csc/{unique}")).await?;
    let busy = session(&harness, project_id, 60).await?;
    for _ in 0..60 {
        chunk(&harness, busy, "prompt", &format!("the {needle} again")).await?;
    }

    let (status, answer) = search(&harness, &format!("q={needle}")).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    let shown = &answer["sessions"][0];
    assert_eq!(shown["match_count"], 60);
    assert_eq!(shown["matches_truncated"], true);
    assert_eq!(shown["matches"].as_array().map(Vec::len), Some(50));
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_ignores_case_in_words_and_text() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let unique = Uuid::new_v4().simple().to_string();
    let project_id = seed_project(harness.handle.pool(), &format!("/csi/{unique}")).await?;
    let found = session(&harness, project_id, 1).await?;
    chunk(&harness, found, "prompt", &format!("École{unique}")).await?;

    let (status, answer) =
        search(&harness, &format!("q=%C3%89COLE{}", unique.to_uppercase())).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(session_ids(&answer), vec![found.to_string()]);
    harness.cleanup().await
}

#[tokio::test]
async fn content_search_past_the_last_page_still_returns_the_totals() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, answer) = search(&harness, &format!("q={}&offset=5", seeded.needle)).await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["total_sessions"], 2, "{answer}");
    assert_eq!(answer["total_matches"], 4, "{answer}");
    assert!(session_ids(&answer).is_empty());
    harness.cleanup().await
}
