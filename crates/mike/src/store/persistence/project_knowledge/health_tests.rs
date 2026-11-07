use super::tests::document;
use super::*;
use crate::store::persistence::{
    PersistHandle,
    lessons::NewLessonWrite,
    test_support::{acquire_ephemeral_db, seed_project},
};
use serde_json::{Value, json};
use uuid::Uuid;

async fn location(handle: &PersistHandle, project: &str, root: &str) -> anyhow::Result<Uuid> {
    let row = handle
        .register_project_location(&LocationInput {
            project: project.into(),
            machine_uuid: Uuid::new_v4(),
            machine_name: root.trim_start_matches('/').replace('/', "-"),
            root_path: root.into(),
        })
        .await?;
    Ok(serde_json::from_value(row["location_uuid"].clone())?)
}

async fn source(handle: &PersistHandle, location: Uuid, name: &str) -> anyhow::Result<Uuid> {
    let row = handle
        .register_knowledge_source(&SourceInput {
            location_uuid: location,
            name: name.into(),
            root_path: format!("/roots/{name}"),
        })
        .await?;
    Ok(serde_json::from_value(row["source_uuid"].clone())?)
}

fn entry(health: &Value, location: Uuid) -> anyhow::Result<&Value> {
    health["locations"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["location"]["location_uuid"] == json!(location))
        })
        .ok_or_else(|| anyhow::anyhow!("location {location} missing from health"))
}

async fn new_lesson(handle: &PersistHandle, project: i64, text: &str) -> anyhow::Result<Uuid> {
    let lesson = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project,
            kind: "warning",
            text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    Ok(lesson.uuid)
}

#[tokio::test]
async fn source_health_reports_fresh_silent_and_failed_locations() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/health-a").await?;
    seed_project(handle.pool(), "/test/health-other").await?;
    let fresh = location(&handle, "/test/health-a", "/work/fresh").await?;
    let silent = location(&handle, "/test/health-a", "/work/silent").await?;
    let failed = location(&handle, "/test/health-a", "/work/failed").await?;
    let empty = location(&handle, "/test/health-a", "/work/empty").await?;
    let outsider = location(&handle, "/test/health-other", "/work/outsider").await?;
    let fresh_source = source(&handle, fresh, "artifacts").await?;
    let silent_source = source(&handle, silent, "artifacts").await?;
    let failed_source = source(&handle, failed, "artifacts").await?;
    source(&handle, outsider, "artifacts").await?;

    let scan = ScanInput {
        generation: 0,
        documents: vec![document("Notebook evidence", "accepted")],
    };
    handle.ingest_knowledge_scan(fresh_source, &scan).await?;
    assert_eq!(
        handle
            .record_knowledge_source_heartbeats(&[fresh_source, silent_source])
            .await?,
        2
    );
    assert_eq!(
        handle
            .record_knowledge_source_heartbeats(&[fresh_source])
            .await?,
        0
    );
    sqlx::query(
        "UPDATE memory.knowledge_sources SET last_heartbeat_at = now() - interval '1 hour'
         WHERE source_uuid = $1",
    )
    .bind(silent_source)
    .execute(handle.pool())
    .await?;
    let error = format!("malformed document design.md\n{}", "x".repeat(4000));
    handle
        .record_knowledge_source_failure(failed_source, &error)
        .await?;

    let health = handle.knowledge_project_health("/test/health-a").await?;
    assert_eq!(health["locations"].as_array().map(Vec::len), Some(4));
    assert!(health["as_of"].is_string());
    assert!(entry(&health, outsider).is_err());

    let fresh_row = &entry(&health, fresh)?["sources"][0];
    assert_eq!(fresh_row["generation"], 1);
    assert!(fresh_row["last_scan_at"].is_string());
    assert!(fresh_row["last_heartbeat_at"].is_string());
    assert!(fresh_row["last_failure"].is_null());
    assert!(fresh_row["last_failure_generation"].is_null());

    let silent_row = &entry(&health, silent)?["sources"][0];
    assert!(silent_row["last_scan_at"].is_null());
    assert_eq!(silent_row["generation"], 0);
    assert!(silent_row["last_heartbeat_at"].is_string());

    let failed_row = &entry(&health, failed)?["sources"][0];
    assert_eq!(failed_row["last_failure"], json!(error));
    assert_eq!(failed_row["last_failure_generation"], 0);
    assert!(failed_row["last_failure_at"].is_string());
    assert!(failed_row["last_heartbeat_at"].is_string());
    assert!(failed_row["last_scan_at"].is_null());

    assert_eq!(entry(&health, empty)?["sources"], json!([]));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn source_health_keeps_a_failure_after_a_later_successful_scan() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/health-retry").await?;
    let loc = location(&handle, "/test/health-retry", "/work/retry").await?;
    let src = source(&handle, loc, "artifacts").await?;
    handle
        .record_knowledge_source_failure(src, "root missing")
        .await?;
    handle
        .ingest_knowledge_scan(
            src,
            &ScanInput {
                generation: 0,
                documents: vec![document("Recovered", "accepted")],
            },
        )
        .await?;
    let health = handle
        .knowledge_project_health("/test/health-retry")
        .await?;
    let row = &entry(&health, loc)?["sources"][0];
    assert_eq!(row["last_failure"], "root missing");
    assert_eq!(row["last_failure_generation"], 0);
    assert_eq!(row["generation"], 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn source_health_rejects_unknown_sources_and_empty_failures() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/health-errors").await?;
    let loc = location(&handle, "/test/health-errors", "/work/errors").await?;
    let src = source(&handle, loc, "artifacts").await?;
    assert_eq!(
        handle
            .record_knowledge_source_heartbeats(&[Uuid::new_v4()])
            .await?,
        0
    );
    assert!(
        handle
            .record_knowledge_source_failure(Uuid::new_v4(), "boom")
            .await
            .is_err()
    );
    assert!(
        handle
            .record_knowledge_source_failure(src, "  \n")
            .await
            .is_err()
    );
    assert!(
        handle
            .knowledge_project_health("/test/no-such-project")
            .await
            .is_err()
    );
    let half_written = sqlx::query(
        "UPDATE memory.knowledge_sources SET last_failure = 'x' WHERE source_uuid = $1",
    )
    .bind(src)
    .execute(handle.pool())
    .await;
    assert!(half_written.is_err());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lesson_health_counts_pending_and_conflicted_per_location() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/health-lessons").await?;
    let other = seed_project(handle.pool(), "/test/health-lessons-other").await?;
    let a = location(&handle, "/test/health-lessons", "/work/a").await?;
    let b = location(&handle, "/test/health-lessons", "/work/b").await?;
    let foreign = location(&handle, "/test/health-lessons-other", "/work/foreign").await?;
    let first = new_lesson(&handle, project, "Keep evidence whole").await?;
    let second = new_lesson(&handle, project, "Keep every body whole").await?;
    let foreign_lesson = new_lesson(&handle, other, "Another project's rule").await?;

    let health = handle
        .knowledge_project_health("/test/health-lessons")
        .await?;
    assert_eq!(
        entry(&health, a)?["lessons"],
        json!({"pending": 2, "conflicted": 0})
    );
    assert_eq!(
        entry(&health, b)?["lessons"],
        json!({"pending": 2, "conflicted": 0})
    );

    handle
        .acknowledge_lesson_files(
            a,
            json!([{"lesson_uuid": first, "version": 1}, {"lesson_uuid": second, "version": 1}]),
        )
        .await?;
    assert_eq!(
        handle
            .record_lesson_file_conflicts(b, &[first, first])
            .await?,
        1
    );
    let health = handle
        .knowledge_project_health("/test/health-lessons")
        .await?;
    assert_eq!(
        entry(&health, a)?["lessons"],
        json!({"pending": 0, "conflicted": 0})
    );
    assert_eq!(
        entry(&health, b)?["lessons"],
        json!({"pending": 2, "conflicted": 1})
    );

    assert!(
        handle
            .record_lesson_file_conflicts(b, &[foreign_lesson])
            .await
            .is_err()
    );
    assert!(
        handle
            .record_lesson_file_conflicts(foreign, &[first])
            .await
            .is_err()
    );
    assert!(
        handle
            .record_lesson_file_conflicts(Uuid::new_v4(), &[first])
            .await
            .is_err()
    );

    handle
        .acknowledge_lesson_files(b, json!([{"lesson_uuid": second, "version": 1}]))
        .await?;
    let health = handle
        .knowledge_project_health("/test/health-lessons")
        .await?;
    assert_eq!(
        entry(&health, b)?["lessons"],
        json!({"pending": 1, "conflicted": 1})
    );

    handle
        .acknowledge_lesson_files(b, json!([{"lesson_uuid": first, "version": 1}]))
        .await?;
    let health = handle
        .knowledge_project_health("/test/health-lessons")
        .await?;
    assert_eq!(
        entry(&health, b)?["lessons"],
        json!({"pending": 0, "conflicted": 0})
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn missing_root_is_recorded_once_and_ignored_for_unregistered_sources() -> anyhow::Result<()>
{
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/health-missing").await?;
    let loc = location(&handle, "/test/health-missing", "/work/missing").await?;
    source(&handle, loc, "docs").await?;
    let roots = [
        SourceInput {
            location_uuid: loc,
            name: "docs".into(),
            root_path: "/roots/docs".into(),
        },
        SourceInput {
            location_uuid: loc,
            name: "never-registered".into(),
            root_path: "/roots/never-registered".into(),
        },
    ];
    assert_eq!(handle.record_knowledge_roots_missing(&roots).await?, 1);
    assert_eq!(handle.record_knowledge_roots_missing(&roots).await?, 0);
    let health = handle
        .knowledge_project_health("/test/health-missing")
        .await?;
    let sources = entry(&health, loc)?["sources"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(sources.len(), 1);
    let failure = sources[0]["last_failure"].as_str().unwrap_or_default();
    assert!(failure.contains("/roots/docs does not exist"), "{failure}");
    assert_eq!(
        sources[0]["last_failure_generation"],
        sources[0]["generation"]
    );
    db.cleanup().await?;
    Ok(())
}

async fn freshness_of(handle: &PersistHandle, project: &str) -> anyhow::Result<(Value, Value)> {
    let found = handle
        .search_project_knowledge(&KnowledgeQuery {
            project: project.into(),
            ..KnowledgeQuery::default()
        })
        .await?;
    let item = found["items"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| anyhow::anyhow!("no indexed document for {project}"))?;
    let reference = item["reference"].as_str().unwrap_or_default().to_owned();
    let read = handle.read_project_knowledge(project, &reference).await?;
    Ok((
        item["provenance"]["freshness"].clone(),
        read["freshness"].clone(),
    ))
}

#[tokio::test]
async fn search_and_read_report_freshness_from_the_source_scan_state() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = "/test/health-freshness";
    seed_project(handle.pool(), project).await?;
    let loc = location(&handle, project, "/work/freshness").await?;
    let src = source(&handle, loc, "docs").await?;
    handle
        .ingest_knowledge_scan(
            src,
            &ScanInput {
                generation: 0,
                documents: vec![document("indexed text", "accepted")],
            },
        )
        .await?;

    let (searched, read) = freshness_of(&handle, project).await?;
    assert_eq!(searched["state"], "fresh");
    assert_eq!(read["state"], "fresh");
    assert_eq!(read["reasons"], json!([]));

    sqlx::query(
        "UPDATE memory.knowledge_sources SET last_scan_at = now() - interval '2 hours'
         WHERE source_uuid = $1",
    )
    .bind(src)
    .execute(handle.pool())
    .await?;
    let (searched, read) = freshness_of(&handle, project).await?;
    for freshness in [searched, read] {
        assert_eq!(freshness["state"], "stale");
        let reasons = freshness["reasons"].to_string();
        assert!(
            reasons.contains("no scanner has checked the source since"),
            "{reasons}"
        );
    }

    handle
        .record_knowledge_source_failure(src, "root vanished")
        .await?;
    let (searched, read) = freshness_of(&handle, project).await?;
    for freshness in [searched, read] {
        assert_eq!(freshness["state"], "stale");
        let reasons = freshness["reasons"].to_string();
        assert!(
            reasons.contains("latest scan failed: root vanished"),
            "{reasons}"
        );
    }
    db.cleanup().await?;
    Ok(())
}
