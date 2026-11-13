use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::cross_crate_tests::api_project_knowledge_rerank_tests::seed_project_with_documents;
use crate::api::project_knowledge::wide::{Pool, candidate_rows, merge};
use crate::api::rerank_search::harness::Harness;
use crate::search_ranking::mark_outside_project;
use crate::store::persistence::SENTINEL_PROJECT_PATH;
use crate::store::persistence::project_knowledge::{
    DocumentInput, KnowledgeQuery, LocationInput, ProjectIdentity, ScanInput, SourceInput,
};
use crate::store::persistence::test_support::seed_project;

const WANTED: i64 = 50;

pub(super) async fn seed_unassigned_document(harness: &Harness) -> anyhow::Result<ProjectIdentity> {
    let handle = &harness.handle;
    seed_project(handle.pool(), SENTINEL_PROJECT_PATH).await?;
    let sentinel = handle
        .knowledge()
        .find_knowledge_project(SENTINEL_PROJECT_PATH)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the (no project) project is missing"))?;
    let location = handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: sentinel.project_uuid.to_string(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "test".into(),
            root_path: "/unassigned".into(),
        })
        .await?;
    let source = handle
        .knowledge()
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: "artifacts".into(),
            root_path: "/unassigned/artifacts".into(),
        })
        .await?;
    let body = "Delta explains how the cache stays warm".to_owned();
    handle
        .knowledge()
        .ingest_knowledge_scan(
            serde_json::from_value(source["source_uuid"].clone())?,
            &ScanInput {
                generation: 0,
                documents: vec![DocumentInput {
                    relative_path: "d.md".into(),
                    title: "Delta".into(),
                    content_hash: Sha256::digest(body.as_bytes())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                    body,
                    kind: "design".into(),
                    status: "accepted".into(),
                    metadata: json!({}),
                }],
            },
        )
        .await?;
    Ok(sentinel)
}

fn query(project: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        project: project.to_owned(),
        query: "cache".into(),
        ..KnowledgeQuery::default()
    }
}

fn references(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| row["reference"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn candidate_rows_with_the_no_project_setting_on_equal_the_sequential_merge()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    let sentinel = seed_unassigned_document(&harness).await?;
    let identity = harness
        .handle
        .knowledge()
        .resolve_knowledge_project(&project)
        .await?;
    let input = query(&project);

    let own = harness
        .handle
        .knowledge()
        .knowledge_candidate_rows(identity.id, &input, WANTED, None)
        .await?;
    let mut extra = harness
        .handle
        .knowledge()
        .knowledge_candidate_rows(sentinel.id, &input, WANTED, None)
        .await?;
    extra.iter_mut().for_each(mark_outside_project);
    let sequential = merge(own, extra, WANTED as usize);
    let pool = Pool {
        project: &identity,
        input: &input,
        wanted: WANTED,
        vector: None,
        reranked: true,
    };
    let together = candidate_rows(&harness.state.knowledge_state(), &harness.handle, pool).await?;

    assert_eq!(references(&together).len(), 4);
    assert_eq!(together, sequential);
    let marked = together
        .iter()
        .filter(|row| row["provenance"]["outside_project"] == true)
        .count();
    assert_eq!(marked, 1);
    harness.cleanup().await
}

#[tokio::test]
async fn candidate_rows_with_the_no_project_setting_off_query_only_the_callers_project()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    seed_unassigned_document(&harness).await?;
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.search_no_project", &json!(false));
    let identity = harness
        .handle
        .knowledge()
        .resolve_knowledge_project(&project)
        .await?;
    let input = query(&project);
    let pool = Pool {
        project: &identity,
        input: &input,
        wanted: WANTED,
        vector: None,
        reranked: true,
    };

    let rows = candidate_rows(&harness.state.knowledge_state(), &harness.handle, pool).await?;

    assert_eq!(references(&rows).len(), 3);
    assert_eq!(
        harness
            .handle
            .knowledge()
            .knowledge_memos()
            .counts_fetched()
            .2,
        0
    );
    harness.cleanup().await
}

#[tokio::test]
async fn candidate_rows_for_several_searches_look_the_no_project_project_up_once()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let project = seed_project_with_documents(&harness).await?;
    seed_unassigned_document(&harness).await?;
    let identity = harness
        .handle
        .knowledge()
        .resolve_knowledge_project(&project)
        .await?;
    let input = query(&project);

    for _ in 0..3 {
        let pool = Pool {
            project: &identity,
            input: &input,
            wanted: WANTED,
            vector: None,
            reranked: true,
        };
        candidate_rows(&harness.state.knowledge_state(), &harness.handle, pool).await?;
    }

    assert_eq!(
        harness
            .handle
            .knowledge()
            .knowledge_memos()
            .counts_fetched()
            .2,
        1
    );
    harness.cleanup().await
}
