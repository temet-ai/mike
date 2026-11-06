use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

const BULK_UPSERT_SQL: &str = "WITH incoming AS (
     SELECT n, p, d
       FROM UNNEST($1::TEXT[], $2::TEXT[], $3::TEXT[]) AS t(n, p, d)
 ),
 file_backed AS (
     SELECT id FROM agents.sources WHERE code = 'file_backed' LIMIT 1
 ),
 observed_source AS (
     SELECT id FROM agents.sources WHERE code = 'observed' LIMIT 1
 ),
 upd AS (
     UPDATE agents.agents a
        SET source_id = CASE
                WHEN a.source_id = obs.id THEN fb.id
                ELSE a.source_id
            END,
            definition_path = COALESCE(a.definition_path, i.p),
            description     = CASE
                WHEN a.description IS NULL
                    THEN i.d
                WHEN a.description ~ '^[>|][-+]?$'
                    THEN i.d
                ELSE a.description
            END,
            updated_at      = NOW()
       FROM incoming i, file_backed fb, observed_source obs
      WHERE a.name = i.n
     RETURNING a.name
 ),
 ins AS (
     INSERT INTO agents.agents (name, definition_path, description, source_id)
     SELECT i.n, i.p, i.d, fb.id
       FROM incoming i, file_backed fb
      WHERE NOT EXISTS (
          SELECT 1 FROM agents.agents a WHERE a.name = i.n
      )
     RETURNING name
 )
 SELECT ((SELECT COUNT(*) FROM upd) + (SELECT COUNT(*) FROM ins))::BIGINT";

async fn run_bulk_upsert(
    pool: &PgPool,
    names: &[String],
    paths: &[String],
    descriptions: &[Option<String>],
) -> anyhow::Result<i64> {
    let inserted: i64 = sqlx::query_scalar(BULK_UPSERT_SQL)
        .bind(names)
        .bind(paths)
        .bind(descriptions)
        .fetch_one(pool)
        .await?;
    Ok(inserted)
}

async fn assert_bulk_upsert_inserts_three_rows(pool: &PgPool, suffix: &str) -> anyhow::Result<()> {
    let names = vec![
        format!("alpha-{suffix}"),
        format!("beta-{suffix}"),
        format!("gamma-{suffix}"),
    ];
    let paths = vec![
        format!("/p/alpha-{suffix}.md"),
        format!("/p/beta-{suffix}.md"),
        format!("/p/gamma-{suffix}.md"),
    ];
    let descriptions = vec![
        Some("alpha desc".to_string()),
        None,
        Some("gamma desc".to_string()),
    ];

    let inserted = run_bulk_upsert(pool, &names, &paths, &descriptions).await?;
    assert_eq!(inserted, 3, "bulk INSERT must report 3 rows affected");

    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT a.name, a.definition_path, a.description, s.code
           FROM agents.agents a
           JOIN agents.sources s ON s.id = a.source_id
          WHERE a.name = ANY($1::TEXT[])
          ORDER BY a.name",
    )
    .bind(&names)
    .fetch_all(pool)
    .await?;
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0],
        (
            names[0].clone(),
            paths[0].clone(),
            Some("alpha desc".to_string()),
            "file_backed".to_string()
        )
    );
    assert_eq!(
        rows[1],
        (
            names[1].clone(),
            paths[1].clone(),
            None,
            "file_backed".to_string()
        )
    );
    assert_eq!(
        rows[2],
        (
            names[2].clone(),
            paths[2].clone(),
            Some("gamma desc".to_string()),
            "file_backed".to_string()
        )
    );
    Ok(())
}

async fn assert_bulk_upsert_promotes_observed(pool: &PgPool, suffix: &str) -> anyhow::Result<()> {
    let name = format!("frontend-developer-{suffix}");
    sqlx::query(
        "INSERT INTO agents.agents (name, source_id)
              VALUES ($1, (SELECT id FROM agents.sources WHERE code = 'observed'))",
    )
    .bind(&name)
    .execute(pool)
    .await?;

    let names = vec![name.clone()];
    let paths = vec![format!(
        "/cwd/.claude/agents/frontend-developer-{suffix}.md"
    )];
    let descriptions = vec![Some("UI work".to_string())];
    run_bulk_upsert(pool, &names, &paths, &descriptions).await?;

    let row: (String, Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT a.name, a.definition_path, a.description, s.code
           FROM agents.agents a
           JOIN agents.sources s ON s.id = a.source_id
          WHERE a.name = $1",
    )
    .bind(&name)
    .fetch_one(pool)
    .await?;
    assert_eq!(row.0, name);
    assert_eq!(
        row.1.as_deref(),
        Some(paths[0].as_str()),
        "blank definition_path must be filled"
    );
    assert_eq!(
        row.2.as_deref(),
        Some("UI work"),
        "NULL description must be filled"
    );
    assert_eq!(row.3, "file_backed", "source must promote from observed");
    Ok(())
}

async fn assert_bulk_upsert_does_not_stomp_dynamic_source(
    pool: &PgPool,
    suffix: &str,
) -> anyhow::Result<()> {
    let name = format!("oneshot-persona-{suffix}");
    sqlx::query(
        "INSERT INTO agents.agents (name, description, source_id)
              VALUES ($1, $2, (SELECT id FROM agents.sources WHERE code = 'dynamic'))",
    )
    .bind(&name)
    .bind("operator-authored")
    .execute(pool)
    .await?;

    let names = vec![name.clone()];
    let paths = vec![format!("/p/oneshot-{suffix}.md")];
    let descriptions = vec![Some("scanner-overwrite-attempt".to_string())];
    run_bulk_upsert(pool, &names, &paths, &descriptions).await?;

    let row: (Option<String>, String) = sqlx::query_as(
        "SELECT a.description, s.code
           FROM agents.agents a
           JOIN agents.sources s ON s.id = a.source_id
          WHERE a.name = $1",
    )
    .bind(&name)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        row.0.as_deref(),
        Some("operator-authored"),
        "operator-edited description must NOT be overwritten"
    );
    assert_eq!(row.1, "dynamic", "dynamic source must NOT be promoted");
    Ok(())
}

async fn assert_bulk_upsert_overwrites_broken_yaml_indicator(
    pool: &PgPool,
    suffix: &str,
) -> anyhow::Result<()> {
    let broken = [">", ">-", ">+", "|", "|-", "|+"];
    for (idx, indicator) in broken.iter().enumerate() {
        sqlx::query(
            "INSERT INTO agents.agents (name, description, source_id)
                  VALUES ($1, $2, (SELECT id FROM agents.sources WHERE code = 'file_backed'))",
        )
        .bind(format!("broken-yaml-{suffix}-{idx}"))
        .bind(*indicator)
        .execute(pool)
        .await?;
    }

    let names: Vec<String> = (0..broken.len())
        .map(|i| format!("broken-yaml-{suffix}-{i}"))
        .collect();
    let paths: Vec<String> = (0..broken.len())
        .map(|i| format!("/p/broken-yaml-{suffix}-{i}.md"))
        .collect();
    let descriptions: Vec<Option<String>> = (0..broken.len())
        .map(|i| Some(format!("real desc {i}")))
        .collect();
    run_bulk_upsert(pool, &names, &paths, &descriptions).await?;

    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, description FROM agents.agents
          WHERE name = ANY($1::TEXT[])
          ORDER BY name",
    )
    .bind(&names)
    .fetch_all(pool)
    .await?;
    assert_eq!(rows.len(), broken.len());
    for (idx, row) in rows.iter().enumerate() {
        assert_eq!(
            row.1.as_deref(),
            Some(format!("real desc {idx}").as_str()),
            "broken YAML indicator must be overwritten with fresh value"
        );
    }
    Ok(())
}

async fn assert_bulk_upsert_preserves_operator_authored_description(
    pool: &PgPool,
    suffix: &str,
) -> anyhow::Result<()> {
    let name = format!("careful-agent-{suffix}");
    sqlx::query(
        "INSERT INTO agents.agents (name, description, source_id)
              VALUES ($1, $2, (SELECT id FROM agents.sources WHERE code = 'file_backed'))",
    )
    .bind(&name)
    .bind("Hand-written notes — do not stomp.")
    .execute(pool)
    .await?;

    let names = vec![name.clone()];
    let paths = vec![format!("/p/careful-{suffix}.md")];
    let descriptions = vec![Some("scanner-overwrite-attempt".to_string())];
    run_bulk_upsert(pool, &names, &paths, &descriptions).await?;

    let row: (Option<String>,) =
        sqlx::query_as("SELECT description FROM agents.agents WHERE name = $1")
            .bind(&name)
            .fetch_one(pool)
            .await?;
    assert_eq!(
        row.0.as_deref(),
        Some("Hand-written notes — do not stomp."),
        "operator-edited description must survive a scan"
    );
    Ok(())
}

async fn assert_bulk_upsert_preserves_existing_definition_path(
    pool: &PgPool,
    suffix: &str,
) -> anyhow::Result<()> {
    let name = format!("path-set-agent-{suffix}");
    sqlx::query(
        "INSERT INTO agents.agents (name, definition_path, source_id)
              VALUES ($1, $2, (SELECT id FROM agents.sources WHERE code = 'file_backed'))",
    )
    .bind(&name)
    .bind("/original/path.md")
    .execute(pool)
    .await?;

    let names = vec![name.clone()];
    let paths = vec![format!("/different/path-{suffix}.md")];
    let descriptions: Vec<Option<String>> = vec![None];
    run_bulk_upsert(pool, &names, &paths, &descriptions).await?;

    let row: (Option<String>,) =
        sqlx::query_as("SELECT definition_path FROM agents.agents WHERE name = $1")
            .bind(&name)
            .fetch_one(pool)
            .await?;
    assert_eq!(
        row.0.as_deref(),
        Some("/original/path.md"),
        "existing definition_path must NOT be overwritten"
    );
    Ok(())
}

#[tokio::test]
async fn bulk_upsert_covers_insert_promote_preserve_and_overwrite_semantics() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping bulk_upsert_covers_insert_promote_preserve_and_overwrite_semantics"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let suffix = uuid::Uuid::new_v4();

    assert_bulk_upsert_inserts_three_rows(&pool, &suffix.to_string()).await?;
    assert_bulk_upsert_promotes_observed(&pool, &suffix.to_string()).await?;
    assert_bulk_upsert_does_not_stomp_dynamic_source(&pool, &suffix.to_string()).await?;
    assert_bulk_upsert_overwrites_broken_yaml_indicator(&pool, &suffix.to_string()).await?;
    assert_bulk_upsert_preserves_operator_authored_description(&pool, &suffix.to_string()).await?;
    assert_bulk_upsert_preserves_existing_definition_path(&pool, &suffix.to_string()).await?;

    test_db.cleanup().await?;
    Ok(())
}
