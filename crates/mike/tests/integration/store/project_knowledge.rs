use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::project_knowledge::{
    DocumentInput, KnowledgeQuery, LessonImport, LocationInput, ScanInput, SourceInput,
};
use mike::store::persistence::work_threads::{CheckpointInput, MetaUpdate, StateFields};
use mike::store::test_support::{TestDb, seed_project, seed_two_sessions};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn document(body: &str, status: &str) -> DocumentInput {
    DocumentInput {
        relative_path: "design.md".into(),
        title: "Notebook design".into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body: body.into(),
        kind: "design".into(),
        status: status.into(),
        metadata: serde_json::json!({}),
    }
}

async fn location(handle: &PersistHandle, project: &str) -> anyhow::Result<Uuid> {
    let row = handle
        .register_project_location(&LocationInput {
            project: project.into(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "desktop".into(),
            root_path: "/work/app".into(),
        })
        .await?;
    Ok(serde_json::from_value(row["location_uuid"].clone())?)
}

async fn assert_later_generations_supersede_then_delete(
    handle: &PersistHandle,
    source: Uuid,
    search: &mike::store::persistence::project_knowledge::KnowledgeQuery,
    reference: &str,
) -> anyhow::Result<()> {
    handle
        .ingest_knowledge_scan(
            source,
            &ScanInput {
                generation: 1,
                documents: vec![document("Notebook replaced", "superseded")],
            },
        )
        .await?;
    assert!(
        handle.search_project_knowledge(search).await?["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert_eq!(
        handle
            .read_project_knowledge(&search.project, reference)
            .await?["body"],
        "Notebook identity evidence"
    );
    handle
        .ingest_knowledge_scan(
            source,
            &ScanInput {
                generation: 2,
                documents: vec![],
            },
        )
        .await?;
    assert_eq!(
        handle
            .read_project_knowledge(&search.project, reference)
            .await?["deleted"],
        true
    );

    Ok(())
}

#[tokio::test]
async fn project_knowledge_locations_and_revision_scope() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let unique = Uuid::new_v4();
    let project_a = format!("/test/knowledge-a-{unique}");
    let project_b = format!("/test/knowledge-b-{unique}");
    let a = seed_project(handle.pool(), &project_a).await?;
    seed_project(handle.pool(), &project_b).await?;
    let desktop = location(&handle, &project_a).await?;
    let mac = location(&handle, &project_a).await?;
    assert_eq!(handle.project_for_location(mac).await?.id, a);
    location(&handle, &project_b).await?;
    let project = handle.project_for_location(desktop).await?;
    let machine: Uuid =
        sqlx::query_scalar("SELECT machine_uuid FROM projects.locations WHERE location_uuid = $1")
            .bind(desktop)
            .fetch_one(handle.pool())
            .await?;
    assert!(
        handle
            .register_project_location(&LocationInput {
                project: project_b.clone(),
                machine_uuid: machine,
                machine_name: "desktop".into(),
                root_path: "/work/app".into(),
            })
            .await
            .is_err()
    );
    let source = handle
        .register_knowledge_source(&SourceInput {
            location_uuid: desktop,
            name: "artifacts".into(),
            root_path: "/work/artifacts".into(),
        })
        .await?;
    let source: Uuid = serde_json::from_value(source["source_uuid"].clone())?;
    let scan = ScanInput {
        generation: 0,
        documents: vec![document("Notebook identity evidence", "accepted")],
    };
    handle.ingest_knowledge_scan(source, &scan).await?;
    assert!(handle.ingest_knowledge_scan(source, &scan).await.is_err());
    let search = mike::store::persistence::project_knowledge::KnowledgeQuery {
        project: project.project_uuid.to_string(),
        query: "notebook".into(),
        ..Default::default()
    };
    let result = handle.search_project_knowledge(&search).await?;
    let reference = result["items"][0]["reference"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing result"))?;
    assert_eq!(
        handle
            .read_project_knowledge(&search.project, reference)
            .await?["body"],
        "Notebook identity evidence"
    );
    assert!(
        handle
            .read_project_knowledge(&project_b, reference)
            .await
            .is_err()
    );
    assert_later_generations_supersede_then_delete(&handle, source, &search, reference).await?;

    handle.pool().close().await;
    Ok(())
}

async fn seed_notebook(handle: &PersistHandle, session: Uuid) -> anyhow::Result<Uuid> {
    let note: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace.session_notes(session_id, kind, title, body)
        SELECT id, 'finding', 'Portable notebook finding', 'Notebook evidence'
        FROM sessions.sessions WHERE session_uuid = $1 RETURNING note_uuid",
    )
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    Ok(note)
}

async fn assert_lesson_file_round_trip(
    handle: &PersistHandle,
    loc: Uuid,
    target: i64,
) -> anyhow::Result<()> {
    let lesson = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: target,
            kind: "warning",
            text: "Keep all evidence",
            topic: Some("notebooks"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let files = handle.lesson_files(loc).await?;
    let mut payload = files["lessons"][0]["payload"].clone();
    assert_eq!(payload["lesson_uuid"], lesson.uuid.to_string());
    payload["body"] = json!("Keep all evidence and revisions");
    let changed = handle
        .import_lesson_file(&LessonImport {
            location_uuid: loc,
            expected_version: 1,
            payload: payload.clone(),
        })
        .await?;
    assert_eq!(changed["version"], 2);
    assert!(
        handle
            .import_lesson_file(&LessonImport {
                location_uuid: loc,
                expected_version: 1,
                payload,
            })
            .await
            .is_err()
    );
    handle.delete_lesson(lesson.id).await?;
    assert_eq!(
        handle.lesson_files(loc).await?["lessons"][0]["payload"]["stage"],
        "archived"
    );
    sqlx::query("DELETE FROM memory.lessons WHERE id = $1")
        .bind(lesson.id)
        .execute(handle.pool())
        .await?;
    assert_eq!(
        handle.lesson_files(loc).await?["lessons"][0]["payload"]["deleted"],
        true
    );

    Ok(())
}

#[tokio::test]
async fn project_knowledge_notebook_binding_and_lesson_round_trip() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let p = seed_project(handle.pool(), "/test/lesson-origin").await?;
    let target = seed_project(handle.pool(), "/test/lesson-target").await?;
    let session = Uuid::new_v4();
    seed_two_sessions(
        handle.pool(),
        &session.to_string(),
        &Uuid::new_v4().to_string(),
        p,
    )
    .await?;
    let loc = location(&handle, "/test/lesson-target").await?;
    let note = seed_notebook(&handle, session).await?;
    handle.bind_session_location(session, loc).await?;
    let note_ref = format!("note:{}", note);
    assert!(
        handle
            .read_project_knowledge("/test/lesson-origin", &note_ref)
            .await
            .is_err()
    );
    assert_eq!(
        handle
            .read_project_knowledge("/test/lesson-target", &note_ref)
            .await?["body"],
        "Notebook evidence"
    );
    let actual: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(actual, target);
    assert_eq!(
        handle
            .resolve_lesson_project(&session.to_string(), "/remote/other-path")
            .await?,
        target
    );
    assert_eq!(
        handle
            .lesson_session_projects()
            .await?
            .get(&session.to_string())
            .map(String::as_str),
        Some("/test/lesson-target")
    );
    assert_lesson_file_round_trip(&handle, loc, target).await?;

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn project_knowledge_work_thread_search_and_read_are_project_scoped() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let suffix = Uuid::new_v4().simple().to_string();
    let path = format!("/test/work-thread-knowledge-{suffix}");
    let other_path = format!("/test/work-thread-other-{suffix}");
    let project = seed_project(handle.pool(), &path).await?;
    let other = seed_project(handle.pool(), &other_path).await?;
    let slug = format!("harbour-{suffix}");
    let checkpoint = CheckpointInput {
        slug: &slug,
        tldr: "Quokka migration summary",
        state: StateFields {
            where_we_are: "Pelican ledger is half built",
            next_up: "Wire the zeppelin adapter",
            blockers: "Waiting on the marmalade key",
            open_questions: "Does the walrus cache survive restarts",
            decisions: "Use the narwhal queue",
            files: &[],
        },
        session_uuid: None,
        commit_sha: None,
        contribution: None,
        meta: MetaUpdate {
            title: Some("Harbour rebuild"),
            goal: Some("Replace the lighthouse"),
            acceptance_criteria: Some("1. The lighthouse is gone"),
            ..MetaUpdate::default()
        },
    };
    handle.checkpoint_work_thread(project, &checkpoint).await?;

    for term in [
        "zeppelin",
        "marmalade",
        "lighthouse",
        "Harbour rebuild",
        "narwhal",
    ] {
        let found = handle
            .search_project_knowledge(&KnowledgeQuery {
                project: path.clone(),
                query: term.into(),
                kind: Some("work_thread".into()),
                ..KnowledgeQuery::default()
            })
            .await?;
        let items = found["items"].as_array().cloned().unwrap_or_default();
        assert_eq!(items.len(), 1, "{term}: {found}");
        assert_eq!(items[0]["reference"], format!("work_thread:{slug}"));
        assert_eq!(items[0]["source_kind"], "work_thread");
        assert_eq!(items[0]["title"], "Harbour rebuild");
    }
    let elsewhere = handle
        .search_project_knowledge(&KnowledgeQuery {
            project: other_path.clone(),
            query: "zeppelin".into(),
            kind: Some("work_thread".into()),
            ..KnowledgeQuery::default()
        })
        .await?;
    assert!(elsewhere["items"].as_array().is_some_and(Vec::is_empty));

    let read = handle
        .read_project_knowledge(&path, &format!("work_thread:{slug}"))
        .await?;
    assert_eq!(read["slug"], slug);
    assert_eq!(read["title"], "Harbour rebuild");
    assert_eq!(read["goal"], "Replace the lighthouse");
    assert_eq!(read["tldr"], "Quokka migration summary");
    assert_eq!(read["where_we_are"], "Pelican ledger is half built");
    assert_eq!(read["next_up"], "Wire the zeppelin adapter");
    assert_eq!(read["blockers"], "Waiting on the marmalade key");
    assert_eq!(
        read["open_questions"],
        "Does the walrus cache survive restarts"
    );
    assert_eq!(read["decisions"], "Use the narwhal queue");
    assert_eq!(read["acceptance_criteria"], "1. The lighthouse is gone");
    assert!(
        handle
            .read_project_knowledge(&other_path, &format!("work_thread:{slug}"))
            .await
            .is_err()
    );
    assert!(
        handle
            .read_project_knowledge(&path, "work_thread:no-such-thread")
            .await
            .is_err()
    );

    sqlx::query("DELETE FROM work.threads WHERE project_id = ANY($1)")
        .bind(vec![project, other])
        .execute(handle.pool())
        .await?;
    db.cleanup().await?;
    Ok(())
}
