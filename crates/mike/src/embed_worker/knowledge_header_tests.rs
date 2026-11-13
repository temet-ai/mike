use super::knowledge_tests::{scan, source};
use crate::api::project_knowledge::item_header;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::project_knowledge::{
    KnowledgeEmbeddingSource, KnowledgeQuery, LocationInput, ProjectIdentity, SourceInput,
};
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_session_turns,
};
use crate::store::persistence::work_threads::{CheckpointInput, MetaUpdate, StateFields};
use std::collections::BTreeSet;
use uuid::Uuid;

const SESSION_TITLE: &str = "Header session";

struct Seeded {
    db: TestDbHandle,
    handle: PersistHandle,
    project: ProjectIdentity,
    session: i64,
    session_uuid: Uuid,
    slug: String,
}

async fn session(
    handle: &PersistHandle,
    project: i64,
    title: &str,
    overview: Option<&str>,
) -> anyhow::Result<(i64, Uuid)> {
    let uuid = Uuid::new_v4();
    let id = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title,
                                        overview, overview_generated_at)
         VALUES ($1, $2, now(), now(), $3, $4, CASE WHEN $4::text IS NULL THEN NULL ELSE now() END)
         RETURNING id",
    )
    .bind(uuid)
    .bind(project)
    .bind(title)
    .bind(overview)
    .fetch_one(handle.pool())
    .await?;
    Ok((id, uuid))
}

async fn named_source(
    handle: &PersistHandle,
    project: &ProjectIdentity,
    name: &str,
) -> anyhow::Result<Uuid> {
    let location = handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: project.project_uuid.to_string(),
            machine_uuid: Uuid::new_v4(),
            machine_name: format!("header-{name}"),
            root_path: format!("/work/{name}"),
        })
        .await?;
    let source = handle
        .knowledge()
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: name.into(),
            root_path: format!("/work/{name}/files"),
        })
        .await?;
    Ok(serde_json::from_value(source["source_uuid"].clone())?)
}

async fn seed_work_thread(
    handle: &PersistHandle,
    project: i64,
    session: Uuid,
) -> anyhow::Result<String> {
    let slug = format!("header-{}", Uuid::new_v4().simple());
    handle
        .launch()
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug: &slug,
                tldr: "Thread summary",
                state: StateFields::default(),
                session_uuid: None,
                commit_sha: None,
                contribution: None,
                meta: MetaUpdate {
                    title: Some("Header thread"),
                    ..MetaUpdate::default()
                },
            },
        )
        .await?;
    sqlx::query(
        "INSERT INTO work.thread_log (thread_id, ord, entry_date, session_uuid, summary, kind, body)
         SELECT id, 9000, current_date, $3, 'Handoff on the adapter', 'handoff', 'The adapter needs a key'
           FROM work.threads WHERE slug = $1 AND project_id = $2",
    )
    .bind(&slug)
    .bind(project)
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok(slug)
}

async fn seed_verdict_and_question(seeded: &Seeded, asker: i64) -> anyhow::Result<()> {
    let pool = seeded.handle.pool();
    seed_session_turns(pool, &seeded.session_uuid.to_string(), seeded.project.id, 1).await?;
    let turn: i64 =
        sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE project_id = $1 LIMIT 1")
            .bind(seeded.project.id)
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             confidence, rationale, unmet_ask, evidence_quality, as_of, input_text, reaction,
             delivery, source, created_at)
         VALUES ($1, $2, 'live', 'r1', 'i1', 'test', 0.9, 'The export was ignored', 'Export the report',
                 'asserted', now(), '', 'corrected', 'partial', 'llm', now())",
    )
    .bind(turn)
    .bind(seeded.session)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO workspace.questions
            (project_id, asker_session_id, to_task, question, state, answer,
             settled_by_session_id, settled_at)
         VALUES ($1, $2, 'task-1', 'Which queue should the seat use?', 'answered',
                 'Use the priority queue', $3, now())",
    )
    .bind(seeded.project.id)
    .bind(asker)
    .bind(seeded.session)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_session_items(seeded: &Seeded, asker: i64) -> anyhow::Result<()> {
    let pool = seeded.handle.pool();
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body)
         VALUES ($1, 'decision', 'Pick the vector form', 'We pick the titled form.')",
    )
    .bind(seeded.session)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic, session_id)
         SELECT $1, lk.id, 'Always run the migration check first', ls.id, lsrc.id,
                'database-migrations', $2
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(seeded.project.id)
    .bind(seeded.session)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.session_knowledge (session_id, topics, learnings, generated_at)
         VALUES ($1, ARRAY['adapters'], ARRAY['Use the second key'], now())",
    )
    .bind(seeded.session_uuid)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body)
         VALUES (gen_random_uuid(), $1, $2, 'The build is green')",
    )
    .bind(seeded.session)
    .bind(asker)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.threads (thread_uuid, subject, project_id, last_message_at)
         VALUES (gen_random_uuid(), 'Release coordination', $1, now() - interval '1 hour')",
    )
    .bind(seeded.project.id)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, thread_id, body)
         SELECT gen_random_uuid(), $2, t.id, 'Coordination notice for the release'
           FROM mailbox.threads t WHERE t.project_id = $1",
    )
    .bind(seeded.project.id)
    .bind(asker)
    .execute(pool)
    .await?;
    seed_verdict_and_question(seeded, asker).await
}

async fn seeded() -> anyhow::Result<Seeded> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Headers", Uuid::new_v4())
        .await?;
    let (session_id, session_uuid) = session(
        &handle,
        project.id,
        SESSION_TITLE,
        Some("The session chose the titled form."),
    )
    .await?;
    let (asker, _) = session(&handle, project.id, "Asker session", None).await?;
    let slug = seed_work_thread(&handle, project.id, session_uuid).await?;
    let seeded = Seeded {
        db,
        handle,
        project,
        session: session_id,
        session_uuid,
        slug,
    };
    seed_session_items(&seeded, asker).await?;
    let artifacts = source(&seeded.handle, &seeded.project.project_uuid.to_string()).await?;
    scan(&seeded.handle, artifacts, 0, "Artifact text").await?;
    let memory = named_source(&seeded.handle, &seeded.project, "claude-memory").await?;
    scan(&seeded.handle, memory, 0, "Memory text").await?;
    Ok(seeded)
}

async fn pending(seeded: &Seeded) -> anyhow::Result<Vec<KnowledgeEmbeddingSource>> {
    Ok(seeded
        .handle
        .knowledge()
        .pending_knowledge_embeddings("no-key")
        .await?)
}

fn header_of<'a>(sources: &'a [KnowledgeEmbeddingSource], kind: &str) -> Vec<&'a str> {
    sources
        .iter()
        .filter(|source| source.source_kind == kind)
        .map(|source| source.header.as_str())
        .collect()
}

async fn item_for(
    seeded: &Seeded,
    source: &KnowledgeEmbeddingSource,
) -> anyhow::Result<serde_json::Value> {
    let words: Vec<&str> = source.body.split_whitespace().rev().take(6).collect();
    let found = seeded
        .handle
        .knowledge()
        .search_project_knowledge(&KnowledgeQuery {
            project: seeded.project.project_uuid.to_string(),
            query: words.join(" "),
            any_term: true,
            kind: matches!(source.source_kind.as_str(), "unmet_ask" | "correction")
                .then(|| source.source_kind.clone()),
            limit: Some(100),
            ..Default::default()
        })
        .await?;
    found["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["reference"] == source.reference.as_str())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("search returned no item for {}", source.reference))
}

#[tokio::test]
async fn pending_header_for_each_kind_is_the_label_then_the_item_or_session_name()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    let sources = pending(&seeded).await?;

    assert_eq!(
        header_of(&sources, "note"),
        ["note, decision: Pick the vector form"]
    );
    assert_eq!(
        header_of(&sources, "lesson"),
        ["lesson, correction, database-migrations: Header session"]
    );
    assert_eq!(header_of(&sources, "session"), ["session: Header session"]);
    assert_eq!(
        header_of(&sources, "learning"),
        ["learning: Header session"]
    );
    assert_eq!(header_of(&sources, "mail"), ["mail: Header session"]);
    assert_eq!(
        header_of(&sources, "unmet_ask"),
        ["unmet_ask: Header session"]
    );
    assert_eq!(
        header_of(&sources, "correction"),
        ["correction: Header session"]
    );
    assert_eq!(
        header_of(&sources, "question_answer"),
        ["question_answer: Header session"]
    );
    assert_eq!(
        header_of(&sources, "thread"),
        ["thread: Release coordination"]
    );
    let entries = header_of(&sources, "thread_entry");
    assert!(!entries.is_empty());
    assert!(
        entries
            .iter()
            .all(|h| *h == format!("thread_entry: {}", seeded.slug))
    );
    let mut artifacts = header_of(&sources, "artifact");
    artifacts.sort_unstable();
    assert_eq!(artifacts, ["artifact: Design", "memory: Design"]);
    seeded.db.cleanup().await
}

#[tokio::test]
async fn pending_header_label_equals_the_reranker_label_of_the_same_search_item()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    let sources = pending(&seeded).await?;
    let mut kinds = BTreeSet::new();
    for source in &sources {
        let item = item_for(&seeded, source).await?;
        let label = item_header::label(&item);
        let name_form = format!("{label}: ");
        assert!(
            source.header == label || source.header.starts_with(&name_form),
            "{} vs {label}",
            source.header
        );
        kinds.insert(source.source_kind.as_str());
    }
    assert_eq!(kinds.len(), 11, "{kinds:?}");
    seeded.db.cleanup().await
}

#[tokio::test]
async fn pending_header_without_any_name_is_the_label_alone_and_long_names_are_cut()
-> anyhow::Result<()> {
    let seeded = seeded().await?;
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'A lesson with no session', ls.id, lsrc.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(seeded.project.id)
    .execute(seeded.handle.pool())
    .await?;
    let long = format!("Words\n\n  with   spaces {}", "x".repeat(300));
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body) VALUES ($1, 'finding', $2, 'b')",
    )
    .bind(seeded.session)
    .bind(&long)
    .execute(seeded.handle.pool())
    .await?;
    let sources = pending(&seeded).await?;

    assert!(header_of(&sources, "lesson").contains(&"lesson, correction"));
    let cut = header_of(&sources, "note")
        .into_iter()
        .find(|h| h.starts_with("note, finding: Words with spaces x"))
        .ok_or_else(|| anyhow::anyhow!("no finding header"))?;
    assert_eq!(cut.chars().count(), 120);
    assert!(!cut.contains("  ") && !cut.contains('\n'));
    seeded.db.cleanup().await
}
