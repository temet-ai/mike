//! Moved from crates/mike/src/api/project_knowledge/rerank_machine_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::{seed_project, seed_session_turns};

struct Seeded {
    path: String,
    turns: Vec<String>,
    brief: String,
    mail: String,
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
    let path = format!("/rerank-machine/{}", Uuid::new_v4().simple());
    let project = seed_project(pool, &path).await?;
    let session = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(session)
    .bind(project)
    .fetch_one(pool)
    .await?;
    let first = add_turn(pool, session, project, "Warm the cache before the release").await?;
    let second = add_turn(pool, session, project, "Explain why the cache goes cold").await?;
    let brief: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body)
         VALUES ($1, 'brief', 'Launch brief: cache', 'cache cache cache seat briefing')
         RETURNING note_uuid",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    let mail: Uuid = sqlx::query_scalar(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body)
         VALUES (gen_random_uuid(), $1, $1, 'cache mail between two seats') RETURNING message_uuid",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(Seeded {
        path,
        turns: vec![format!("turn:{first}"), format!("turn:{second}")],
        brief: format!("note:{brief}"),
        mail: format!("mail:{mail}"),
    })
}

fn references(answer: &Value) -> Vec<String> {
    answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn knowledge_search_rerank_puts_a_brief_note_below_operator_turns_and_leaves_mail_in_the_reranker_order()
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
    assert_eq!(answer["rerank"]["state"], "reranked");
    let found = references(&answer);
    assert_eq!(found.len(), 4, "{answer}");
    assert_eq!(found[3], seeded.brief);
    for turn in &seeded.turns {
        assert!(found[..3].contains(turn), "{turn} in {found:?}");
    }
    let find = |reference: &str| {
        answer["items"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["reference"] == reference))
            .ok_or_else(|| anyhow::anyhow!("{reference} missing from {answer}"))
    };
    let brief = find(&seeded.brief)?;
    assert_eq!(brief["provenance"]["written_by"], "machine");
    assert_eq!(brief["provenance"]["ranked_lower"], true);
    let mail = find(&seeded.mail)?;
    assert_eq!(mail["provenance"]["written_by"], "machine");
    assert!(mail["provenance"]["ranked_lower"].is_null());
    let scores: Vec<f64> = answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item["reference"] != seeded.brief)
                .filter_map(|item| item["rerank_score"].as_f64())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(scores.len(), 3, "{answer}");
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_rerank_with_only_operator_turns_keeps_the_reranker_order()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;
    let route = format!(
        "/api/knowledge/search?project={}&query=cache&kind=turn",
        seeded.path
    );

    let (status, answer) = harness.get(&route).await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(answer["rerank"]["state"], "reranked");
    let mut plain = seeded.turns.clone();
    plain.sort();
    let mut found = references(&answer);
    assert_eq!(found.len(), 2);
    let reranked = found.clone();
    found.sort();
    assert_eq!(found, plain);
    let scores: Vec<f64> = answer["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["rerank_score"].as_f64())
                .collect()
        })
        .unwrap_or_default();
    assert!(scores[0] > scores[1], "{reranked:?} {scores:?}");
    harness.cleanup().await
}
