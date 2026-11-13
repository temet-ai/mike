use super::{Fixture, fixture};

const CLEANUP_MIGRATION: &str =
    include_str!("../../../../../../mike-base/src/store/persistence/migrations/20261111170900_work_threads_cleanup.sql");

type ThreadRow = (String, Option<String>, Option<String>, Option<String>);

async fn restore_pre_migration_shape(f: &Fixture) -> anyhow::Result<()> {
    let pool = f.handle.pool();
    sqlx::raw_sql(
        "ALTER TABLE work.threads DROP CONSTRAINT threads_status_check;
         ALTER TABLE work.threads ADD CONSTRAINT threads_status_check
             CHECK (status IN ('active', 'implementing', 'review', 'parked', 'done', 'complete', 'in-progress'));
         CREATE TABLE steering.thread_acceptance_criteria (
             thread_slug   text PRIMARY KEY,
             criteria_text text NULL,
             source_path   text NOT NULL,
             parsed_at     timestamptz NOT NULL DEFAULT now(),
             parse_error   text NULL);",
    )
    .execute(pool)
    .await?;
    for (slug, title, status) in [
        ("cleanup-a", "Thread: Alpha work", "complete"),
        ("cleanup-b", "thread:", "in-progress"),
        ("cleanup-c", "Plain title", "active"),
    ] {
        sqlx::query(
            "INSERT INTO work.threads (project_id, slug, title, status) VALUES ($1, $2, $3, $4)",
        )
        .bind(f.project)
        .bind(slug)
        .bind(title)
        .bind(status)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO work.thread_acceptance_criteria (thread_id, criteria_text)
         SELECT id, 'already in work' FROM work.threads WHERE slug = 'cleanup-c'",
    )
    .execute(pool)
    .await?;
    sqlx::raw_sql(
        "INSERT INTO steering.thread_acceptance_criteria (thread_slug, criteria_text, source_path)
         VALUES ('cleanup-a', 'copied from steering', '/w/a'),
                ('cleanup-c', 'older steering copy', '/w/c'),
                ('cleanup-orphan', 'no such thread', '/w/o')",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn threads(f: &Fixture) -> anyhow::Result<Vec<ThreadRow>> {
    Ok(sqlx::query_as::<_, ThreadRow>(
        "SELECT t.slug, t.title, t.status, c.criteria_text
           FROM work.threads t
           LEFT JOIN work.thread_acceptance_criteria c ON c.thread_id = t.id
          WHERE t.slug LIKE 'cleanup-%'
          ORDER BY t.slug",
    )
    .fetch_all(f.handle.pool())
    .await?)
}

fn expected() -> Vec<ThreadRow> {
    let row = |slug: &str, title: &str, status: &str, criteria: Option<&str>| {
        (
            slug.to_owned(),
            Some(title.to_owned()),
            Some(status.to_owned()),
            criteria.map(str::to_owned),
        )
    };
    vec![
        row(
            "cleanup-a",
            "Alpha work",
            "done",
            Some("copied from steering"),
        ),
        row("cleanup-b", "cleanup-b", "implementing", None),
        row(
            "cleanup-c",
            "Plain title",
            "active",
            Some("already in work"),
        ),
    ]
}

#[tokio::test]
async fn cleanup_migration_strips_labels_maps_statuses_and_retires_the_steering_table()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    restore_pre_migration_shape(&f).await?;
    sqlx::raw_sql(CLEANUP_MIGRATION)
        .execute(f.handle.pool())
        .await?;
    assert_eq!(threads(&f).await?, expected());
    let steering: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('steering.thread_acceptance_criteria')::text")
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(steering, None, "the steering copy must be dropped");

    sqlx::raw_sql(CLEANUP_MIGRATION)
        .execute(f.handle.pool())
        .await?;
    assert_eq!(
        threads(&f).await?,
        expected(),
        "a second run changes nothing"
    );

    let legacy = sqlx::query(
        "INSERT INTO work.threads (project_id, slug, title, status) VALUES ($1, 'cleanup-z', 'z', 'complete')",
    )
    .bind(f.project)
    .execute(f.handle.pool())
    .await;
    assert!(
        legacy.is_err(),
        "a legacy status must be refused after the migration"
    );
    Ok(())
}
