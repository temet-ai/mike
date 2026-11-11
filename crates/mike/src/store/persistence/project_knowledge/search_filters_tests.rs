use serde_json::Value;
use uuid::Uuid;

use super::KnowledgeQuery;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDb, TestDbHandle, seed_project};

struct Seeded {
    _db: TestDbHandle,
    handle: PersistHandle,
    project: String,
    word: String,
    slug: String,
    note_in_window: String,
    note_later: String,
    note_tagged: String,
    older_lesson: String,
    newer_lesson: String,
    superseded_lesson: String,
}

async fn session(handle: &PersistHandle, project: i64, title: &str) -> anyhow::Result<(i64, Uuid)> {
    let uuid = Uuid::new_v4();
    let id = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(uuid)
    .bind(project)
    .bind(title)
    .fetch_one(handle.pool())
    .await?;
    Ok((id, uuid))
}

async fn note(
    handle: &PersistHandle,
    (session, title, tags, at): (i64, &str, Vec<String>, &str),
) -> anyhow::Result<String> {
    let uuid: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body, tags, updated_at)
         VALUES ($1, 'finding', $2, $2, $3, $4::timestamptz) RETURNING note_uuid",
    )
    .bind(session)
    .bind(title)
    .bind(tags)
    .bind(at)
    .fetch_one(handle.pool())
    .await?;
    Ok(format!("note:{uuid}"))
}

async fn lesson(
    handle: &PersistHandle,
    project: i64,
    session: i64,
    text: &str,
) -> anyhow::Result<(i64, String)> {
    let (id, uuid): (i64, Uuid) = sqlx::query_as(
        "INSERT INTO memory.lessons (project_id, session_id, kind_id, text, stage_id)
         VALUES ($1, $2, 1, $3, 2) RETURNING id, uuid",
    )
    .bind(project)
    .bind(session)
    .bind(text)
    .fetch_one(handle.pool())
    .await?;
    Ok((id, format!("lesson:{uuid}")))
}

async fn relate(
    handle: &PersistHandle,
    newer: i64,
    older: i64,
    kind: i64,
    status: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lesson_relations (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, detector_version)
         VALUES ($1, $2, $3, 4, $4, 'search-filters-test')",
    )
    .bind(newer)
    .bind(older)
    .bind(kind)
    .bind(status)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn thread(handle: &PersistHandle, project: i64, worked_by: Uuid) -> anyhow::Result<String> {
    let slug = format!("release-{}", Uuid::new_v4().simple());
    sqlx::query(
        "WITH t AS (INSERT INTO work.threads (project_id, slug, title) VALUES ($1, $2, 'Release thread') RETURNING id)
         INSERT INTO work.thread_sessions (thread_id, session_uuid) SELECT id, $3 FROM t",
    )
    .bind(project)
    .bind(&slug)
    .bind(worked_by)
    .execute(handle.pool())
    .await?;
    Ok(slug)
}

async fn seed() -> anyhow::Result<Option<Seeded>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let path = format!("/test/knowledge-filters-{}", Uuid::new_v4());
    let project = seed_project(handle.pool(), &path).await?;
    let word = format!("cormorant{}", Uuid::new_v4().simple());
    let (release, release_uuid) = session(&handle, project, "Release batch for the Mac").await?;
    let (other, _) = session(&handle, project, "Unrelated work").await?;
    let slug = thread(&handle, project, release_uuid).await?;
    let note_in_window = note(
        &handle,
        (
            release,
            &format!("{word} window note"),
            vec![],
            "2026-11-03T12:00:00Z",
        ),
    )
    .await?;
    let note_later = note(
        &handle,
        (
            other,
            &format!("{word} later note"),
            vec![],
            "2026-11-08T12:00:00Z",
        ),
    )
    .await?;
    let tag = vec![format!("thread:{slug}")];
    let note_tagged = note(
        &handle,
        (
            other,
            &format!("{word} tagged note"),
            tag,
            "2026-11-09T12:00:00Z",
        ),
    )
    .await?;
    let (older_id, older_lesson) = lesson(
        &handle,
        project,
        other,
        &format!("{word} deploy after each PR"),
    )
    .await?;
    let (newer_id, newer_lesson) = lesson(
        &handle,
        project,
        release,
        &format!("{word} deploy once per batch"),
    )
    .await?;
    relate(&handle, newer_id, older_id, 4, 1).await?;
    let (gone_id, superseded_lesson) =
        lesson(&handle, project, other, &format!("{word} lock the build")).await?;
    relate(&handle, newer_id, gone_id, 1, 2).await?;
    let project: Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(project)
            .fetch_one(handle.pool())
            .await?;
    Ok(Some(Seeded {
        _db: db,
        handle,
        project: project.to_string(),
        word,
        slug,
        note_in_window,
        note_later,
        note_tagged,
        older_lesson,
        newer_lesson,
        superseded_lesson,
    }))
}

impl Seeded {
    fn query(&self, kind: &str) -> KnowledgeQuery {
        KnowledgeQuery {
            project: self.project.clone(),
            query: self.word.clone(),
            kind: Some(kind.to_owned()),
            ..KnowledgeQuery::default()
        }
    }

    async fn references(&self, input: KnowledgeQuery) -> anyhow::Result<Vec<String>> {
        let found = self.handle.search_project_knowledge(&input).await?;
        let mut references: Vec<String> = found["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i["reference"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        references.sort();
        Ok(references)
    }

    async fn item(&self, input: KnowledgeQuery, reference: &str) -> anyhow::Result<Value> {
        let found = self.handle.search_project_knowledge(&input).await?;
        found["items"]
            .as_array()
            .and_then(|items| items.iter().find(|i| i["reference"] == reference).cloned())
            .ok_or_else(|| anyhow::anyhow!("{reference} not in {found}"))
    }
}

fn sorted(mut references: Vec<String>) -> Vec<String> {
    references.sort();
    references
}

#[tokio::test]
async fn search_with_a_date_range_returns_only_items_inside_it() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let all = s.references(s.query("note")).await?;
    assert_eq!(
        all,
        sorted(vec![
            s.note_in_window.clone(),
            s.note_later.clone(),
            s.note_tagged.clone()
        ])
    );

    let window = KnowledgeQuery {
        since: Some("2026-11-02".into()),
        until: Some("2026-11-04".into()),
        ..s.query("note")
    };
    assert_eq!(s.references(window).await?, vec![s.note_in_window.clone()]);

    let since_only = KnowledgeQuery {
        since: Some("2026-11-08T00:00:00Z".into()),
        ..s.query("note")
    };
    assert_eq!(
        s.references(since_only).await?,
        sorted(vec![s.note_later.clone(), s.note_tagged.clone()])
    );
    Ok(())
}

#[tokio::test]
async fn search_with_a_thread_returns_its_sessions_and_tagged_notes_only() -> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let in_thread = KnowledgeQuery {
        thread: Some(s.slug.clone()),
        ..s.query("note")
    };
    assert_eq!(
        s.references(in_thread).await?,
        sorted(vec![s.note_in_window.clone(), s.note_tagged.clone()])
    );

    let lessons = KnowledgeQuery {
        thread: Some(s.slug.clone()),
        ..s.query("lesson")
    };
    assert_eq!(s.references(lessons).await?, vec![s.newer_lesson.clone()]);
    Ok(())
}

#[tokio::test]
async fn search_with_an_unknown_thread_or_a_bad_date_is_refused_naming_the_input()
-> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let unknown = KnowledgeQuery {
        thread: Some("no-such-thread".into()),
        ..s.query("note")
    };
    let Err(refusal) = s.handle.search_project_knowledge(&unknown).await else {
        anyhow::bail!("an unknown thread must be refused");
    };
    assert!(
        refusal.to_string().contains("\"no-such-thread\""),
        "{refusal}"
    );

    let bad = KnowledgeQuery {
        since: Some("11/03/2026".into()),
        ..s.query("note")
    };
    let Err(refusal) = s.handle.search_project_knowledge(&bad).await else {
        anyhow::bail!("a bad date must be refused");
    };
    assert!(refusal.to_string().contains("\"11/03/2026\""), "{refusal}");
    Ok(())
}

#[tokio::test]
async fn search_marks_a_lesson_a_later_lesson_contradicts_and_names_its_author_session()
-> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    let older = s.item(s.query("lesson"), &s.older_lesson).await?;
    assert_eq!(
        older["provenance"]["contradicted_by"]["reference"],
        s.newer_lesson.as_str()
    );
    assert_eq!(older["provenance"]["contradicted_by"]["status"], "proposed");
    assert_eq!(older["author"]["session_title"], "Unrelated work");
    assert!(older["updated_at"].is_string());

    let newer = s.item(s.query("lesson"), &s.newer_lesson).await?;
    assert_eq!(
        newer["author"]["session_title"],
        "Release batch for the Mac"
    );
    assert!(newer["provenance"].get("contradicted_by").is_none());
    Ok(())
}

#[tokio::test]
async fn search_leaves_out_a_superseded_lesson_unless_historical_and_then_marks_it()
-> anyhow::Result<()> {
    let Some(s) = seed().await? else {
        return Ok(());
    };
    assert!(
        !s.references(s.query("lesson"))
            .await?
            .contains(&s.superseded_lesson)
    );

    let historical = KnowledgeQuery {
        include_historical: true,
        ..s.query("lesson")
    };
    let gone = s.item(historical, &s.superseded_lesson).await?;
    assert_eq!(
        gone["provenance"]["superseded_by"]["reference"],
        s.newer_lesson.as_str()
    );
    assert_eq!(gone["provenance"]["superseded_by"]["status"], "confirmed");
    Ok(())
}
