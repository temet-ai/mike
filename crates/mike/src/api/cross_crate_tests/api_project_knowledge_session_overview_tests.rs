//! Moved from crates/mike/src/api/project_knowledge/session_overview_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::seed_project;

const TITLES: [&str; 2] = ["Cormorant adapter work", "Cormorant retry cleanup"];

struct Seeded {
    project: String,
    sessions: Vec<Uuid>,
}

async fn seed(harness: &Harness) -> anyhow::Result<Seeded> {
    let pool = harness.handle.pool();
    let path = format!("/overview-route/{}", Uuid::new_v4());
    let project_id = seed_project(pool, &path).await?;
    let mut sessions = Vec::new();
    for title in TITLES {
        let uuid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title,
                                            overview, overview_generated_at)
             VALUES ($1, $2, now(), now(), $3, $4, now())",
        )
        .bind(uuid)
        .bind(project_id)
        .bind(title)
        .bind(format!("{title}: the cormorant adapter and its second key"))
        .execute(pool)
        .await?;
        sessions.push(uuid);
    }
    Ok(Seeded {
        project: path.replace('/', "%2F"),
        sessions,
    })
}

fn search_route(seeded: &Seeded, extra: &str) -> String {
    format!(
        "/api/knowledge/search?project={}&query=cormorant&kind=session{extra}",
        seeded.project
    )
}

fn read_route(seeded: &Seeded, reference: &str) -> String {
    format!(
        "/api/knowledge/read?project={}&reference={reference}",
        seeded.project
    )
}

#[tokio::test]
async fn read_route_opens_a_session_overview_and_answers_409_for_an_unknown_session()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (status, read) = harness
        .get(&read_route(
            &seeded,
            &format!("session:{}", seeded.sessions[0]),
        ))
        .await?;
    let (missing, error) = harness
        .get(&read_route(&seeded, &format!("session:{}", Uuid::new_v4())))
        .await?;

    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["session_title"], TITLES[0]);
    assert!(
        read["overview"]
            .as_str()
            .is_some_and(|o| o.contains("second key"))
    );
    assert_eq!(read["turn_count"], 0);
    assert_eq!(missing, StatusCode::CONFLICT, "{error}");
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_for_session_overviews_sends_the_reranker_a_session_header()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;
    harness.enable_rerank(800);
    harness.reranker_prefers_later_documents(None).await;

    let (status, answer) = harness.get(&search_route(&seeded, "")).await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(answer["rerank"]["state"], "reranked");
    let sent = harness.rerank_requests().await?;
    let documents: Vec<String> = sent[0]["documents"]
        .as_array()
        .map(|documents| {
            documents
                .iter()
                .filter_map(|d| d.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(documents.len(), 2);
    for document in &documents {
        let mut lines = document.lines();
        let header = lines.next().unwrap_or_default();
        assert!(header.starts_with("session | "), "{header}");
        assert!(TITLES.iter().any(|t| header.ends_with(&format!(" | {t}"))));
        assert!(TITLES.contains(&lines.next().unwrap_or_default()));
    }
    harness.cleanup().await
}

#[tokio::test]
async fn knowledge_search_detail_best_shows_a_later_session_overview_under_the_session_label()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let seeded = seed(&harness).await?;

    let (_, answer) = harness.get(&search_route(&seeded, "&detail=best")).await?;

    let items = answer["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 2, "{answer}");
    let line = items[1]["line"].as_str().unwrap_or_default();
    assert!(line.starts_with("session \u{b7} "), "{line}");
    assert!(TITLES.iter().any(|t| line.contains(t)), "{line}");
    assert!(line.ends_with(" \u{b7} machine-written"), "{line}");
    assert!(line.chars().count() <= 120);
    let first: &Value = &items[0];
    assert_eq!(first["source_kind"], "session");
    harness.cleanup().await
}
