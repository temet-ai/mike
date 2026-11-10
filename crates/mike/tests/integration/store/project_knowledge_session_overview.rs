use super::knowledge_search_fixture::{Fixture, Session, query, references};
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use mike::store::test_support::seed_project;
use serde_json::{Value, json};
use uuid::Uuid;

const OVERVIEW: &str =
    "The session wired the cormorant adapter to the second key and removed the old retry loop.";

async fn set_overview(
    fx: &Fixture,
    session: &Session,
    (title, overview): (Option<&str>, Option<&str>),
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE sessions.sessions
            SET title = $2, overview = $3,
                overview_generated_at = CASE WHEN $3::text IS NULL THEN NULL ELSE now() END
          WHERE id = $1",
    )
    .bind(session.id)
    .bind(title)
    .bind(overview)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

async fn set_knowledge(
    fx: &Fixture,
    session: &Session,
    (topics, learnings): (&[&str], &[&str]),
) -> anyhow::Result<()> {
    let topics: Vec<String> = topics.iter().map(|t| (*t).to_owned()).collect();
    let learnings: Vec<String> = learnings.iter().map(|t| (*t).to_owned()).collect();
    sqlx::query(
        "INSERT INTO memory.session_knowledge (session_id, topics, learnings, generated_at)
         VALUES ($1, $2, $3, now())",
    )
    .bind(session.uuid)
    .bind(topics)
    .bind(learnings)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

async fn titled_session(fx: &Fixture, title: &str, overview: &str) -> anyhow::Result<Session> {
    let session = fx.session().await?;
    set_overview(fx, &session, (Some(title), Some(overview))).await?;
    Ok(session)
}

fn kind_query(text: &str, kind: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        kind: Some(kind.to_owned()),
        ..query(text)
    }
}

#[tokio::test]
async fn session_overview_search_matching_the_overview_returns_reference_kind_title_and_machine_mark()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Cormorant adapter work", OVERVIEW).await?;
    set_knowledge(&fx, &session, (&["adapters", "keys"], &[])).await?;

    let items = fx.search(query("cormorant")).await?;

    assert_eq!(
        references(&items),
        vec![format!("session:{}", session.uuid)]
    );
    let item = &items[0];
    assert_eq!(item["source_kind"], "session");
    assert_eq!(item["title"], "Cormorant adapter work");
    assert!(item["updated_at"].is_string(), "{item}");
    assert_eq!(item["author"]["role"], "judge");
    assert_eq!(item["author"]["session_title"], "Cormorant adapter work");
    assert_eq!(item["provenance"]["written_by"], "machine");
    assert!(item["provenance"].get("ranked_lower").is_none(), "{item}");
    assert_eq!(item["provenance"]["topics"], json!(["adapters", "keys"]));
    assert!(
        item["excerpt"]
            .as_str()
            .is_some_and(|e| e.contains("cormorant"))
    );
    fx.finish().await
}

#[tokio::test]
async fn session_overview_search_without_a_session_title_titles_the_item_with_the_first_80_characters()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    set_overview(&fx, &session, (None, Some(OVERVIEW))).await?;

    let items = fx.search(query("cormorant")).await?;

    let expected: String = OVERVIEW.chars().take(80).collect();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["title"], expected.as_str());
    assert_eq!(items[0]["author"]["session_title"], Value::Null);
    fx.finish().await
}

#[tokio::test]
async fn session_overview_search_with_the_session_kind_returns_only_overviews() -> anyhow::Result<()>
{
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Adapter work", OVERVIEW).await?;
    let note = fx
        .note(
            &session,
            "result",
            ("Cormorant note", "The cormorant adapter note"),
        )
        .await?;

    let only = fx.search(kind_query("cormorant", "session")).await?;
    let all = references(&fx.search(query("cormorant")).await?);

    assert_eq!(references(&only), vec![format!("session:{}", session.uuid)]);
    assert!(
        all.contains(&format!("session:{}", session.uuid)),
        "{all:?}"
    );
    assert!(all.contains(&format!("note:{note}")), "{all:?}");
    assert!(
        fx.search(kind_query("cormorant", "learning"))
            .await?
            .is_empty()
    );
    fx.finish().await
}

#[tokio::test]
async fn session_overview_search_skips_blank_overviews_and_other_projects() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let titled_only = fx.session().await?;
    set_overview(&fx, &titled_only, (Some("Cormorant adapter"), None)).await?;
    let blank = fx.session().await?;
    set_overview(&fx, &blank, (Some("Cormorant blank"), Some("   "))).await?;
    let other_path = format!("/test/overview-other-{}", Uuid::new_v4().simple());
    let other_project = seed_project(fx.handle.pool(), &other_path).await?;
    let other_session: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title, overview,
                                        overview_generated_at)
         VALUES ($1, $2, now(), now(), 'Other', $3, now()) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(other_project)
    .bind(OVERVIEW)
    .fetch_one(fx.handle.pool())
    .await?;

    let items = fx.search(kind_query("cormorant", "session")).await?;

    assert!(items.is_empty(), "{items:?} {other_session}");
    fx.finish().await
}

#[tokio::test]
async fn read_project_knowledge_opens_a_session_overview_with_topics_learnings_and_activity()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Cormorant adapter work", OVERVIEW).await?;
    set_knowledge(
        &fx,
        &session,
        (
            &["adapters", "keys"],
            &["Use the second key", "Drop the retry loop"],
        ),
    )
    .await?;
    fx.turn(&session, "first ask", "first answer").await?;
    fx.turn(&session, "second ask", "second answer").await?;
    let reference = format!("session:{}", session.uuid);

    let read = fx
        .handle
        .read_project_knowledge(Some(&fx.path), &reference)
        .await?;

    assert_eq!(read["reference"], reference.as_str());
    assert_eq!(read["overview"], OVERVIEW);
    assert_eq!(read["topics"], json!(["adapters", "keys"]));
    assert_eq!(
        read["learnings"],
        json!(["Use the second key", "Drop the retry loop"])
    );
    assert_eq!(read["session_title"], "Cormorant adapter work");
    assert_eq!(read["session_uuid"], session.uuid.to_string());
    assert!(read["first_activity"].is_string() && read["last_activity"].is_string());
    assert_eq!(read["turn_count"], 2);
    assert!(
        read["body"]
            .as_str()
            .is_some_and(|b| b.starts_with(OVERVIEW) && b.ends_with("Topics: adapters, keys"))
    );
    assert!(read["project_uuid"].is_string());
    fx.finish().await
}

#[tokio::test]
async fn read_project_knowledge_completes_a_session_reference_given_by_its_first_characters()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Adapter work", OVERVIEW).await?;
    let full = format!("session:{}", session.uuid);
    let short = format!("session:{}", &session.uuid.to_string()[..8]);

    let read = fx
        .handle
        .read_project_knowledge(Some(&fx.path), &short)
        .await?;

    assert_eq!(read["reference"], full.as_str());
    assert_eq!(read["reference_corrected"], full.as_str());
    fx.finish().await
}

#[tokio::test]
async fn read_project_knowledge_with_a_bare_session_uuid_still_opens_the_learning()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Adapter work", OVERVIEW).await?;
    set_knowledge(&fx, &session, (&["adapters"], &["Use the second key"])).await?;

    let read = fx
        .handle
        .read_project_knowledge(Some(&fx.path), &session.uuid.to_string())
        .await?;

    assert_eq!(read["reference"], format!("learning:{}", session.uuid));
    fx.finish().await
}

#[tokio::test]
async fn read_project_knowledge_rejects_a_session_reference_without_an_overview_or_in_another_project()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let blank = fx.session().await?;
    let session = titled_session(&fx, "Adapter work", OVERVIEW).await?;
    let other_path = format!("/test/overview-read-{}", Uuid::new_v4().simple());
    seed_project(fx.handle.pool(), &other_path).await?;

    for (project, reference) in [
        (Some(fx.path.as_str()), format!("session:{}", blank.uuid)),
        (
            Some(fx.path.as_str()),
            format!("session:{}", Uuid::new_v4()),
        ),
        (None, format!("session:{}", blank.uuid)),
        (
            Some(other_path.as_str()),
            format!("session:{}", session.uuid),
        ),
    ] {
        assert!(
            fx.handle
                .read_project_knowledge(project, &reference)
                .await
                .is_err(),
            "{project:?} {reference}"
        );
    }
    fx.finish().await
}

type SourceRow = (String, String, Option<chrono::DateTime<chrono::Utc>>);

async fn source_rows(fx: &Fixture, view: &str) -> anyhow::Result<Vec<SourceRow>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT source_kind, reference, source_updated_at FROM {view}
          WHERE project_id = $1 ORDER BY reference"
    )))
    .bind(fx.project)
    .fetch_all(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn embedding_sources_view_does_not_list_a_session_while_the_view_with_sessions_does()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = titled_session(&fx, "Adapter work", OVERVIEW).await?;
    set_knowledge(&fx, &session, (&["adapters", "keys"], &[])).await?;
    let without_topics = titled_session(&fx, "Plain work", "A plain overview about herons").await?;
    let note = fx
        .note(&session, "result", ("A note", "A note body"))
        .await?;

    let old = source_rows(&fx, "memory.knowledge_embedding_sources_v").await?;
    let new = source_rows(&fx, "memory.knowledge_sources_with_sessions_v").await?;

    assert!(old.iter().all(|row| row.0 != "session"), "{old:?}");
    assert!(
        old.iter().any(|row| row.1 == format!("note:{note}")),
        "{old:?}"
    );
    let sessions: Vec<&SourceRow> = new.iter().filter(|row| row.0 == "session").collect();
    let mut expected = vec![
        format!("session:{}", session.uuid),
        format!("session:{}", without_topics.uuid),
    ];
    expected.sort();
    assert_eq!(
        sessions.iter().map(|row| row.1.clone()).collect::<Vec<_>>(),
        expected
    );
    assert!(sessions.iter().all(|row| row.2.is_some()));
    let others: Vec<&SourceRow> = new.iter().filter(|row| row.0 != "session").collect();
    assert_eq!(others.len(), old.len());
    assert!(others.iter().zip(&old).all(|(a, b)| **a == *b));
    fx.finish().await
}
