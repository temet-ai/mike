use crate::test_db;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

const CONSOLIDATION_SQL: &str = include_str!(
    "../../../mike-base/src/store/persistence/migrations/20260722010000_consolidate_agent_aliases.sql"
);

async fn assert_builtin_source_row_present(pool: &PgPool) -> anyhow::Result<()> {
    let builtin_code: Option<String> =
        sqlx::query_scalar("SELECT code FROM agents.sources WHERE id = 4")
            .fetch_optional(pool)
            .await?;
    assert_eq!(builtin_code.as_deref(), Some("builtin"));
    Ok(())
}

async fn assert_legacy_aliases_table_absent(pool: &PgPool) -> anyhow::Result<()> {
    let aliases_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('agents.aliases')::text")
            .fetch_one(pool)
            .await?;
    assert_eq!(
        aliases_table, None,
        "agents.aliases must be physically removed, not hidden"
    );
    Ok(())
}

async fn assert_alias_columns_are_typed_arrays(pool: &PgPool) -> anyhow::Result<()> {
    let alias_columns: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT column_name, udt_name, is_nullable, column_default
           FROM information_schema.columns
          WHERE table_schema = 'agents'
            AND table_name = 'agents'
            AND column_name IN ('persona_aliases', 'name_variants')
          ORDER BY column_name",
    )
    .fetch_all(pool)
    .await?;
    assert_eq!(
        alias_columns,
        vec![
            (
                "name_variants".to_string(),
                "_text".to_string(),
                "NO".to_string(),
                Some("'{}'::text[]".to_string())
            ),
            (
                "persona_aliases".to_string(),
                "_text".to_string(),
                "NO".to_string(),
                Some("'{}'::text[]".to_string())
            ),
        ]
    );
    Ok(())
}

async fn assert_uuid_identity_column_present(pool: &PgPool) -> anyhow::Result<()> {
    let uuid_column_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM information_schema.columns
          WHERE table_schema = 'agents'
            AND table_name = 'agents'
            AND column_name = 'uuid'
            AND udt_name = 'uuid'
            AND is_nullable = 'NO'",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(uuid_column_count, 1, "the used uuid identity must remain");
    Ok(())
}

async fn assert_dead_metadata_columns_absent(pool: &PgPool) -> anyhow::Result<()> {
    let dead_column_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM information_schema.columns
          WHERE table_schema = 'agents'
            AND table_name = 'agents'
            AND column_name = ANY($1::text[])",
    )
    .bind(
        &[
            "alias",
            "frontmatter",
            "frontmatter_name",
            "frontmatter_description",
            "frontmatter_model",
            "frontmatter_skills",
            "skills",
        ][..],
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(dead_column_count, 0, "dead metadata columns must be gone");
    Ok(())
}

async fn assert_typed_alias_arrays_round_trip(pool: &PgPool) -> anyhow::Result<()> {
    let name = format!("typed-alias-probe-{}", uuid::Uuid::new_v4());
    let row = sqlx::query(
        "INSERT INTO agents.agents (name, persona_aliases, name_variants)
         VALUES ($1, ARRAY['A persona'], ARRAY['probe-name'])
         RETURNING persona_aliases, name_variants",
    )
    .bind(&name)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        row.get::<Vec<String>, _>("persona_aliases"),
        vec!["A persona"]
    );
    assert_eq!(
        row.get::<Vec<String>, _>("name_variants"),
        vec!["probe-name"]
    );
    Ok(())
}

async fn assert_typed_alias_arrays_reject_null_elements(pool: &PgPool) -> anyhow::Result<()> {
    let name = format!("null-alias-probe-{}", uuid::Uuid::new_v4());
    let null_element = sqlx::query(
        "INSERT INTO agents.agents (name, persona_aliases)
         VALUES ($1, ARRAY[NULL]::text[])",
    )
    .bind(&name)
    .execute(pool)
    .await;
    assert!(
        null_element.is_err(),
        "typed alias arrays must reject NULL elements"
    );
    Ok(())
}

#[tokio::test]
async fn agent_identity_uses_one_table_with_typed_alias_arrays() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping agent identity schema test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    assert_builtin_source_row_present(&pool).await?;
    assert_legacy_aliases_table_absent(&pool).await?;
    assert_alias_columns_are_typed_arrays(&pool).await?;
    assert_uuid_identity_column_present(&pool).await?;
    assert_dead_metadata_columns_absent(&pool).await?;
    assert_typed_alias_arrays_round_trip(&pool).await?;
    assert_typed_alias_arrays_reject_null_elements(&pool).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn consolidation_backfills_both_alias_kinds_and_drops_legacy_shape() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping agent alias backfill test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    sqlx::raw_sql(
        "ALTER TABLE agents.agents ADD COLUMN alias text;
         CREATE TABLE agents.aliases (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             agent_id bigint NOT NULL REFERENCES agents.agents(id) ON DELETE CASCADE,
             alias text NOT NULL,
             kind text NOT NULL CHECK (kind IN ('persona', 'name_variant')),
             created_at timestamptz DEFAULT now() NOT NULL
         );",
    )
    .execute(&pool)
    .await?;

    let agent_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.agents (name, alias)
         VALUES ('migration-probe', 'legacy-short-name') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO agents.aliases (agent_id, alias, kind) VALUES
         ($1, 'Full persona sentence', 'persona'),
         ($1, 'alternate-agent-name', 'name_variant')",
    )
    .bind(agent_id)
    .execute(&pool)
    .await?;

    sqlx::raw_sql(CONSOLIDATION_SQL)
        .execute(&pool)
        .await
        .context("consolidation migration must accept the legacy schema")?;

    let row = sqlx::query(
        "SELECT persona_aliases, name_variants
           FROM agents.agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row.get::<Vec<String>, _>("persona_aliases"),
        vec!["Full persona sentence"]
    );
    assert_eq!(
        row.get::<Vec<String>, _>("name_variants"),
        vec!["legacy-short-name", "alternate-agent-name"]
    );

    let aliases_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('agents.aliases')::text")
            .fetch_one(&pool)
            .await?;
    assert_eq!(aliases_table, None);

    sqlx::raw_sql(CONSOLIDATION_SQL)
        .execute(&pool)
        .await
        .context("consolidation migration must be safely re-runnable")?;
    let rerun_personas: Vec<String> =
        sqlx::query_scalar("SELECT persona_aliases FROM agents.agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(rerun_personas, vec!["Full persona sentence"]);

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
