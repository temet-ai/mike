use super::knowledge::{model_key, sweep};
use super::knowledge_tests::{breaker, embedding_server, source};
use crate::store::persistence::{
    PersistHandle, project_knowledge::KnowledgeQuery, test_support::acquire_ephemeral_db,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn angled_literal(similarity: f64) -> String {
    let mut vector = vec![0.0_f64; 768];
    vector[0] = similarity;
    vector[1] = (1.0 - similarity * similarity).sqrt();
    let parts: Vec<String> = vector.iter().map(|v| format!("{v:.6}")).collect();
    format!("[{}]", parts.join(","))
}

async fn scan_titles(
    handle: &PersistHandle,
    source: Uuid,
    documents: &[(&str, &str)],
) -> anyhow::Result<()> {
    let documents: Vec<Value> = documents
        .iter()
        .enumerate()
        .map(|(index, (title, body))| {
            json!({"relative_path": format!("doc-{index}.md"), "title": title, "kind": "design",
                "status": "accepted", "metadata": {}, "body": body,
                "content_hash": Sha256::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()})
        })
        .collect();
    handle
        .ingest_knowledge_scan(
            source,
            &serde_json::from_value(json!({"generation": 0, "documents": documents}))?,
        )
        .await?;
    Ok(())
}

async fn set_similarity(
    handle: &PersistHandle,
    title: &str,
    similarity: f64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.knowledge_embedding_chunks c
            SET embedding = $1::text::vector
           FROM memory.knowledge_embedding_state e
           JOIN memory.knowledge_revisions r ON e.source_kind = 'artifact' AND e.source_id = r.id
          WHERE c.state_id = e.id AND r.title = $2",
    )
    .bind(angled_literal(similarity))
    .bind(title)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn other_project(handle: &PersistHandle, documents: usize) -> anyhow::Result<Vec<String>> {
    let titles: Vec<String> = (0..documents)
        .map(|index| format!("other-{index}"))
        .collect();
    if documents == 0 {
        return Ok(titles);
    }
    let other = handle
        .define_knowledge_project("Other", Uuid::new_v4())
        .await?;
    let other_source = source(handle, &other.project_uuid.to_string()).await?;
    let pairs: Vec<(&str, &str)> = titles
        .iter()
        .map(|title| (title.as_str(), "filler body text from another project"))
        .collect();
    scan_titles(handle, other_source, &pairs).await?;
    Ok(titles)
}

struct Fixture {
    db: crate::store::persistence::test_support::TestDbHandle,
    handle: PersistHandle,
    project: String,
    embedding: (String, String, String),
}

async fn fixture(documents: &[(&str, &str, f64)]) -> anyhow::Result<Fixture> {
    fixture_beside_other_project(documents, 0).await
}

async fn fixture_beside_other_project(
    documents: &[(&str, &str, f64)],
    other_documents: usize,
) -> anyhow::Result<Fixture> {
    fixture_with_lesson(documents, other_documents, None).await
}

async fn insert_lesson(handle: &PersistHandle, project_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, 'floor'
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(project_id)
    .bind("a lesson body that shares no words with the query")
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn set_lesson_similarity(handle: &PersistHandle, similarity: f64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.knowledge_embedding_chunks c
            SET embedding = $1::text::vector
           FROM memory.knowledge_embedding_state e
          WHERE c.state_id = e.id AND e.source_kind = 'lesson'",
    )
    .bind(angled_literal(similarity))
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn place_vectors(
    handle: &PersistHandle,
    documents: &[(&str, &str, f64)],
    other_titles: &[String],
    lesson_similarity: Option<f64>,
) -> anyhow::Result<()> {
    for (title, _, similarity) in documents {
        set_similarity(handle, title, *similarity).await?;
    }
    if let Some(similarity) = lesson_similarity {
        set_lesson_similarity(handle, similarity).await?;
    }
    for title in other_titles {
        set_similarity(handle, title, 0.95).await?;
    }
    Ok(())
}

async fn fixture_with_lesson(
    documents: &[(&str, &str, f64)],
    other_documents: usize,
    lesson_similarity: Option<f64>,
) -> anyhow::Result<Fixture> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("Floor", Uuid::new_v4())
        .await?;
    let source_uuid = source(&handle, &project.project_uuid.to_string()).await?;
    let titles: Vec<(&str, &str)> = documents.iter().map(|(t, b, _)| (*t, *b)).collect();
    scan_titles(&handle, source_uuid, &titles).await?;
    let other_titles = other_project(&handle, other_documents).await?;
    if lesson_similarity.is_some() {
        insert_lesson(&handle, project.id).await?;
    }
    let (_server, service) = embedding_server().await;
    sweep(&handle, &service, &mut breaker()).await?;
    place_vectors(&handle, documents, &other_titles, lesson_similarity).await?;
    sqlx::query("ANALYZE memory.knowledge_embedding_state")
        .execute(handle.pool())
        .await?;
    let embedding = (
        angled_literal(1.0),
        model_key(&service),
        service.model_name(),
    );
    Ok(Fixture {
        db,
        handle,
        project: project.project_uuid.to_string(),
        embedding,
    })
}

async fn titles_for(fixture: &Fixture, query: &str) -> anyhow::Result<Vec<String>> {
    titles_for_kind(fixture, query, None).await
}

async fn titles_for_kind(
    fixture: &Fixture,
    query: &str,
    kind: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let input = KnowledgeQuery {
        project: fixture.project.clone(),
        query: query.into(),
        kind: kind.map(str::to_owned),
        ..Default::default()
    };
    let found = fixture
        .handle
        .search_project_knowledge_hybrid(&input, Some(&fixture.embedding))
        .await?;
    let mut titles: Vec<String> = found["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["title"].as_str().map(str::to_owned))
        .collect();
    titles.sort();
    Ok(titles)
}

#[tokio::test]
async fn semantic_search_drops_hits_more_than_the_window_below_the_best_hit() -> anyhow::Result<()>
{
    let fx = fixture(&[
        (
            "best",
            "alpha body text that has no overlap with the query",
            0.84,
        ),
        (
            "inside",
            "bravo body text that has no overlap with the query",
            0.78,
        ),
        (
            "outside",
            "charlie body text that has no overlap with the query",
            0.70,
        ),
    ])
    .await?;
    assert_eq!(
        titles_for(&fx, "unrelated wording").await?,
        ["best", "inside"]
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn semantic_search_drops_a_best_hit_below_the_absolute_floor() -> anyhow::Result<()> {
    let fx = fixture(&[
        (
            "weak",
            "alpha body text that has no overlap with the query",
            0.55,
        ),
        (
            "weaker",
            "bravo body text that has no overlap with the query",
            0.52,
        ),
    ])
    .await?;
    assert!(titles_for(&fx, "unrelated wording").await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn semantic_search_lexical_match_below_the_floor_is_returned_beside_the_semantic_hit()
-> anyhow::Result<()> {
    let fx = fixture(&[
        (
            "best",
            "alpha body text that has no overlap with the query",
            0.84,
        ),
        (
            "keyword",
            "this body mentions the marker lexicalonly exactly once",
            0.20,
        ),
    ])
    .await?;
    assert_eq!(titles_for(&fx, "lexicalonly").await?, ["best", "keyword"]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn semantic_search_for_a_project_with_few_chunks_still_returns_them() -> anyhow::Result<()> {
    let fx = fixture(&[(
        "only",
        "alpha body text that has no overlap with the query",
        0.91,
    )])
    .await?;
    assert_eq!(titles_for(&fx, "unrelated wording").await?, ["only"]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn semantic_search_small_project_beside_a_large_corpus_still_returns_its_hits()
-> anyhow::Result<()> {
    let fx = fixture_beside_other_project(
        &[(
            "mine",
            "alpha body text that has no overlap with the query",
            0.72,
        )],
        8,
    )
    .await?;
    assert_eq!(titles_for(&fx, "unrelated wording").await?, ["mine"]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn semantic_search_kind_filter_measures_the_window_from_the_best_hit_of_that_kind()
-> anyhow::Result<()> {
    let fx = fixture_with_lesson(
        &[(
            "strong",
            "alpha body text that has no overlap with the query",
            0.90,
        )],
        0,
        Some(0.78),
    )
    .await?;
    assert_eq!(titles_for(&fx, "unrelated wording").await?, ["strong"]);
    assert_eq!(
        titles_for_kind(&fx, "unrelated wording", Some("lesson")).await?,
        ["correction"]
    );
    fx.db.cleanup().await?;
    Ok(())
}
