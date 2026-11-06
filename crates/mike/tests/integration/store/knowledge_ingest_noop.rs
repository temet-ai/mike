use mike::store::persistence::PersistHandle;
use mike::store::persistence::project_knowledge::{
    DocumentInput, LocationInput, ScanInput, SourceInput,
};
use mike::store::test_support::{TestDb, seed_project};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn document(body: &str, metadata: serde_json::Value) -> DocumentInput {
    DocumentInput {
        relative_path: "design.md".into(),
        title: "Design".into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body: body.into(),
        kind: "design".into(),
        status: "accepted".into(),
        metadata,
    }
}

async fn scan(
    handle: &PersistHandle,
    source: Uuid,
    generation: i64,
    doc: DocumentInput,
) -> anyhow::Result<()> {
    handle
        .ingest_knowledge_scan(
            source,
            &ScanInput {
                generation,
                documents: vec![doc],
            },
        )
        .await?;
    Ok(())
}

async fn revisions(handle: &PersistHandle, source: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_revisions r
         JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
         WHERE d.source_uuid = $1",
    )
    .bind(source)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn knowledge_scan_ignores_git_keys_but_records_real_changes() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = format!("/test/knowledge-noop-{}", Uuid::new_v4());
    seed_project(handle.pool(), &project).await?;
    let location = handle
        .register_project_location(&LocationInput {
            project,
            machine_uuid: Uuid::new_v4(),
            machine_name: "desktop".into(),
            root_path: "/work/app".into(),
        })
        .await?;
    let location: Uuid = serde_json::from_value(location["location_uuid"].clone())?;
    let source = handle
        .register_knowledge_source(&SourceInput {
            location_uuid: location,
            name: "artifacts".into(),
            root_path: "/work/artifacts".into(),
        })
        .await?;
    let source: Uuid = serde_json::from_value(source["source_uuid"].clone())?;

    let git = |commit: &str, dirty: bool| serde_json::json!({"owner": "a", "source_git_commit": commit, "source_worktree_dirty": dirty});
    scan(&handle, source, 0, document("body one", git("aaa", false))).await?;
    assert_eq!(revisions(&handle, source).await?, 1);
    scan(&handle, source, 1, document("body one", git("bbb", true))).await?;
    assert_eq!(revisions(&handle, source).await?, 1);
    scan(&handle, source, 2, document("body two", git("bbb", true))).await?;
    assert_eq!(revisions(&handle, source).await?, 2);
    scan(
        &handle,
        source,
        3,
        document(
            "body two",
            serde_json::json!({"owner": "b", "source_git_commit": "bbb"}),
        ),
    )
    .await?;
    assert_eq!(revisions(&handle, source).await?, 3);

    handle.pool().close().await;
    Ok(())
}
