use crate::test_db::TestDb;

async fn insert_project(pool: &sqlx::PgPool, path: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW())
         ON CONFLICT (project_path) DO UPDATE SET last_seen = NOW()
         RETURNING id",
    )
    .bind(path)
    .fetch_one(pool)
    .await?)
}

async fn insert_session(
    pool: &sqlx::PgPool,
    project_id: i64,
    cwd: Option<&str>,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, cwd, first_seen, last_seen)
             VALUES ($1, $2, $3, NOW(), NOW()) RETURNING id",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(project_id)
    .bind(cwd)
    .fetch_one(pool)
    .await?)
}

async fn thread_with_message(
    pool: &sqlx::PgPool,
    project_id: i64,
    from: i64,
    to: i64,
) -> anyhow::Result<i64> {
    let thread_id: i64 = sqlx::query_scalar(
        "INSERT INTO mailbox.threads (thread_uuid, subject, project_id)
             VALUES ($1, 'placement', $2) RETURNING id",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body, thread_id)
             VALUES ($1, $2, $3, 'hello', $4)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(from)
    .bind(to)
    .bind(thread_id)
    .execute(pool)
    .await?;
    Ok(thread_id)
}

async fn thread_project(pool: &sqlx::PgPool, thread_id: i64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT project_id FROM mailbox.threads WHERE id = $1")
            .bind(thread_id)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn thread_under_placeholder_project_takes_its_participants_project() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    let unknown = insert_project(&pool, "(no project)").await?;
    let real = insert_project(&pool, &format!("/work/placed-{}", uuid::Uuid::new_v4())).await?;
    let lost = insert_session(&pool, unknown, None).await?;
    let known = insert_session(&pool, real, None).await?;

    let thread_id = thread_with_message(&pool, unknown, known, lost).await?;

    assert_eq!(thread_project(&pool, thread_id).await?, Some(real));
    Ok(())
}

#[tokio::test]
async fn thread_whose_sessions_have_no_project_is_placed_by_working_folder() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    let unknown = insert_project(&pool, "(no project)").await?;
    let root = format!("/work/folder-{}", uuid::Uuid::new_v4());
    let real = insert_project(&pool, &root).await?;
    let inside = insert_session(&pool, unknown, Some(&format!("{root}/sub/dir"))).await?;
    let nowhere = insert_session(&pool, unknown, None).await?;

    let thread_id = thread_with_message(&pool, unknown, nowhere, inside).await?;

    assert_eq!(thread_project(&pool, thread_id).await?, Some(real));
    Ok(())
}

#[tokio::test]
async fn tags_are_stored_lower_case_and_bare_numbers_become_issues() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    let normalized: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT mailbox.normalize_tag(c)
           FROM UNNEST(ARRAY['AC2', ' Heads Up ', '154', '#154', '---', '']) WITH ORDINALITY AS u(c, n)
          ORDER BY n",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        normalized,
        vec![
            Some("ac2".to_string()),
            Some("heads-up".to_string()),
            Some("issue-154".to_string()),
            Some("issue-154".to_string()),
            None,
            None,
        ]
    );

    let rejected = sqlx::query("INSERT INTO mailbox.tags (code) VALUES ('152')")
        .execute(&pool)
        .await;
    assert!(rejected.is_err(), "a bare number is not a tag");
    Ok(())
}
