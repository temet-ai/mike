use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::project_knowledge::{
    DocumentInput, KnowledgeQuery, LocationInput, ScanInput, SourceInput,
};
use mike::store::persistence::work_threads::{CheckpointInput, MetaUpdate, StateFields};
use mike::store::test_support::{
    TestDb, TestDbHandle, seed_project, seed_session_turns, seed_two_sessions,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

struct Memory {
    db: TestDbHandle,
    handle: PersistHandle,
    path: String,
    other_path: String,
    session: Uuid,
    entry: i64,
    question: Uuid,
    verdict: i64,
}

async fn session_id(handle: &PersistHandle, session: Uuid) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?,
    )
}

async fn seed_thread_entry(
    handle: &PersistHandle,
    project: i64,
    session: Uuid,
) -> anyhow::Result<i64> {
    let slug = format!("memory-{}", Uuid::new_v4().simple());
    handle
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
                    title: Some("Memory thread"),
                    ..MetaUpdate::default()
                },
            },
        )
        .await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO work.thread_log (thread_id, ord, entry_date, session_uuid, summary, kind, body)
         SELECT id, 9000, current_date, $3, 'Handoff on the cormorant adapter', 'handoff',
                'The cormorant adapter needs the second key'
           FROM work.threads WHERE slug = $1 AND project_id = $2
         RETURNING id",
    )
    .bind(&slug)
    .bind(project)
    .bind(session)
    .fetch_one(handle.pool())
    .await?)
}

async fn seed_question(
    handle: &PersistHandle,
    project: i64,
    asker: i64,
    settler: i64,
) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO workspace.questions
            (project_id, asker_session_id, to_task, question, state, answer,
             settled_by_session_id, settled_at)
         VALUES ($1, $2, 'task-1', 'Which ocelot queue should the seat use?', 'answered',
                 'Use the ocelot priority queue', $3, now())
         RETURNING question_uuid",
    )
    .bind(project)
    .bind(asker)
    .bind(settler)
    .fetch_one(handle.pool())
    .await?)
}

async fn seed_verdicts(handle: &PersistHandle, project: i64, session: i64) -> anyhow::Result<i64> {
    let turn: i64 =
        sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE project_id = $1 LIMIT 1")
            .bind(project)
            .fetch_one(handle.pool())
            .await?;
    let insert = "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             confidence, rationale, unmet_ask, evidence_quality, as_of, input_text, reaction,
             delivery, source, created_at)
         VALUES ($1, $2, 'live', 'r1', 'i1', 'test', 0.9, $3, $4, 'asserted', now(), '',
                 'corrected', 'partial', 'llm', $5)
         RETURNING id";
    sqlx::query(insert)
        .bind(turn)
        .bind(session)
        .bind("An older verdict about the wombat")
        .bind("Older wombat ask")
        .bind(chrono::Utc::now() - chrono::Duration::days(1))
        .execute(handle.pool())
        .await?;
    Ok(sqlx::query_scalar(insert)
        .bind(turn)
        .bind(session)
        .bind("The assistant ignored the pangolin export request")
        .bind("Export the pangolin report")
        .bind(chrono::Utc::now())
        .fetch_one(handle.pool())
        .await?)
}

async fn seed_learning_and_mail(
    handle: &PersistHandle,
    session: Uuid,
    from: i64,
    to: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.session_knowledge (session_id, topics, learnings, generated_at)
         VALUES ($1, ARRAY['herons'], ARRAY['Herons need the gannet flag set', 'Second learning'], now())",
    )
    .bind(session)
    .execute(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body)
         VALUES (gen_random_uuid(), $1, $2, 'Starling report: the build is green'),
                (gen_random_uuid(), NULL, $2, '[mike digest] Starling notice')",
    )
    .bind(from)
    .bind(to)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn memory() -> anyhow::Result<Option<Memory>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let suffix = Uuid::new_v4().simple().to_string();
    let path = format!("/test/session-memory-{suffix}");
    let other_path = format!("/test/session-memory-other-{suffix}");
    let project = seed_project(handle.pool(), &path).await?;
    seed_project(handle.pool(), &other_path).await?;
    let (session, peer) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(
        handle.pool(),
        &session.to_string(),
        &peer.to_string(),
        project,
    )
    .await?;
    seed_session_turns(handle.pool(), &session.to_string(), project, 1).await?;
    let (session_db, peer_db) = (
        session_id(&handle, session).await?,
        session_id(&handle, peer).await?,
    );
    let entry = seed_thread_entry(&handle, project, session).await?;
    let question = seed_question(&handle, project, session_db, peer_db).await?;
    let verdict = seed_verdicts(&handle, project, session_db).await?;
    seed_learning_and_mail(&handle, session, session_db, peer_db).await?;
    Ok(Some(Memory {
        db,
        handle,
        path,
        other_path,
        session,
        entry,
        question,
        verdict,
    }))
}

async fn search(
    memory: &Memory,
    project: &str,
    query: &str,
    kind: &str,
) -> anyhow::Result<Vec<Value>> {
    let found = memory
        .handle
        .search_project_knowledge(&KnowledgeQuery {
            project: project.to_owned(),
            query: query.into(),
            kind: Some(kind.into()),
            ..KnowledgeQuery::default()
        })
        .await?;
    Ok(found["items"].as_array().cloned().unwrap_or_default())
}

async fn assert_kind_found_and_read(
    memory: &Memory,
    query: &str,
    kind: &str,
    reference: &str,
    excerpt_word: &str,
) -> anyhow::Result<Value> {
    let items = search(memory, &memory.path, query, kind).await?;
    assert_eq!(items.len(), 1, "{kind}: {items:?}");
    let item = &items[0];
    assert_eq!(item["reference"], reference);
    assert_eq!(item["source_kind"], kind);
    assert!(item["updated_at"].is_string(), "{kind} has a date: {item}");
    let excerpt = item["excerpt"].as_str().unwrap_or_default();
    assert!(excerpt.contains(excerpt_word), "{kind} excerpt {excerpt:?}");
    assert!(excerpt.chars().count() <= 400, "{kind} excerpt is short");
    assert!(
        search(memory, &memory.other_path, query, kind)
            .await?
            .is_empty()
    );
    assert_full_read(memory, reference, excerpt_word).await?;
    Ok(item.clone())
}

async fn assert_full_read(
    memory: &Memory,
    reference: &str,
    excerpt_word: &str,
) -> anyhow::Result<()> {
    let read = memory
        .handle
        .read_project_knowledge(None, reference)
        .await?;
    assert_eq!(read["reference"], reference);
    assert!(
        read["body"]
            .as_str()
            .is_some_and(|b| b.contains(excerpt_word)),
        "{read}"
    );
    assert!(read["project_uuid"].is_string());
    assert!(
        memory
            .handle
            .read_project_knowledge(Some(&memory.other_path), reference)
            .await
            .is_err()
    );
    Ok(())
}

async fn finish(memory: Memory) -> anyhow::Result<()> {
    memory.handle.pool().close().await;
    memory.db.cleanup().await
}

#[tokio::test]
async fn thread_entry_search_finds_entry_with_session_author() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let reference = format!("thread_entry:{}", memory.entry);
    let item = assert_kind_found_and_read(
        &memory,
        "cormorant",
        "thread_entry",
        &reference,
        "cormorant",
    )
    .await?;
    assert_eq!(item["title"], "Handoff on the cormorant adapter");
    assert_eq!(item["author"]["role"], "session");
    assert_eq!(item["author"]["session_uuid"], memory.session.to_string());
    finish(memory).await
}

#[tokio::test]
async fn question_answer_search_finds_answered_question() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let reference = format!("question_answer:{}", memory.question);
    let item =
        assert_kind_found_and_read(&memory, "ocelot", "question_answer", &reference, "ocelot")
            .await?;
    assert_eq!(item["title"], "Which ocelot queue should the seat use?");
    finish(memory).await
}

#[tokio::test]
async fn unmet_ask_search_uses_latest_live_verdict_only() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let reference = format!("unmet_ask:{}", memory.verdict);
    let item = assert_kind_found_and_read(&memory, "pangolin", "unmet_ask", &reference, "pangolin")
        .await?;
    assert_eq!(item["title"], "Export the pangolin report");
    assert_eq!(item["author"]["role"], "judge");
    assert!(
        search(&memory, &memory.path, "wombat", "unmet_ask")
            .await?
            .is_empty()
    );
    finish(memory).await
}

#[tokio::test]
async fn correction_search_finds_corrected_turn_verdict() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let reference = format!("correction:{}", memory.verdict);
    let item =
        assert_kind_found_and_read(&memory, "pangolin", "correction", &reference, "pangolin")
            .await?;
    assert_eq!(item["status"], "corrected");
    let read = memory
        .handle
        .read_project_knowledge(Some(&memory.path), &reference)
        .await?;
    assert_eq!(read["current"], true);
    assert!(
        read["turn_reference"]
            .as_str()
            .is_some_and(|r| r.starts_with("turn:"))
    );
    finish(memory).await
}

#[tokio::test]
async fn learning_search_finds_session_learning() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let reference = format!("learning:{}", memory.session);
    let item =
        assert_kind_found_and_read(&memory, "gannet", "learning", &reference, "gannet").await?;
    assert_eq!(item["title"], "Herons need the gannet flag set");
    finish(memory).await
}

#[tokio::test]
async fn mail_search_finds_session_mail_and_skips_mike_notices() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let items = search(&memory, &memory.path, "starling", "mail").await?;
    assert_eq!(items.len(), 1, "{items:?}");
    let reference = items[0]["reference"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let item =
        assert_kind_found_and_read(&memory, "starling", "mail", &reference, "Starling").await?;
    assert_eq!(item["author"]["session_uuid"], memory.session.to_string());
    finish(memory).await
}

#[tokio::test]
async fn read_without_project_rejects_unknown_reference() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let missing = format!("mail:{}", Uuid::new_v4());
    assert!(
        memory
            .handle
            .read_project_knowledge(None, &missing)
            .await
            .is_err()
    );
    assert!(
        memory
            .handle
            .read_project_knowledge(None, "promise:1")
            .await
            .is_err()
    );
    finish(memory).await
}

fn memory_file() -> DocumentInput {
    let body = "Keep the albatross ledger in one place";
    DocumentInput {
        relative_path: "albatross.md".into(),
        title: "Albatross ledger".into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body: body.into(),
        kind: "memory".into(),
        status: "accepted".into(),
        metadata: serde_json::json!({}),
    }
}

async fn index_on_machine(handle: &PersistHandle, path: &str) -> anyhow::Result<()> {
    let location = handle
        .register_project_location(&LocationInput {
            project: path.into(),
            machine_uuid: Uuid::new_v4(),
            machine_name: "machine".into(),
            root_path: format!("/work/{}", Uuid::new_v4()),
        })
        .await?;
    let source = handle
        .register_knowledge_source(&SourceInput {
            location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
            name: "claude-memory".into(),
            root_path: "/home/memory".into(),
        })
        .await?;
    handle
        .ingest_knowledge_scan(
            serde_json::from_value(source["source_uuid"].clone())?,
            &ScanInput {
                generation: 0,
                documents: vec![memory_file()],
            },
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn memory_file_on_three_machines_is_one_result_with_copies() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    for _ in 0..3 {
        index_on_machine(&memory.handle, &memory.path).await?;
    }
    let items = search(&memory, &memory.path, "albatross", "memory").await?;
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["copies"], 3);
    finish(memory).await
}

#[tokio::test]
async fn lesson_title_is_its_first_line() -> anyhow::Result<()> {
    let Some(memory) = memory().await? else {
        return Ok(());
    };
    let project = memory
        .handle
        .resolve_knowledge_project(&memory.path)
        .await?
        .id;
    memory
        .handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id: project,
            kind: "warning",
            text: "Mind the kestrel cache\nIt drops writes on restart.",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let items = search(&memory, &memory.path, "kestrel", "lesson").await?;
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["title"], "Mind the kestrel cache");
    assert_eq!(items[0]["provenance"]["lesson_kind"], "warning");
    finish(memory).await
}
