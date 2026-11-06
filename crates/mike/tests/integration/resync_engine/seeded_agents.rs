use crate::test_db;

use super::support::{BASELINE_TABLES, baseline_cursors, connect, count};
use mike::resync::ResyncEngine;
use sqlx::PgPool;
use test_db::TestDb;

struct DivergedAgentFixture {
    seeded_name: &'static str,
    dest_uuid_before: String,
    new_agent_name: String,
    expected_personas: Vec<String>,
    expected_name_variants: Vec<String>,
}

async fn diverge_dest_agents_and_seed_new_agent(
    source: &PgPool,
    dest: &PgPool,
) -> anyhow::Result<DivergedAgentFixture> {
    let diverged: u64 = sqlx::query("UPDATE agents.agents SET uuid = gen_random_uuid()")
        .execute(dest)
        .await?
        .rows_affected();
    assert!(
        diverged > 0,
        "dest must carry migration-seeded agents to diverge"
    );

    let seeded_name = "refocus";
    let dest_uuid_before: String =
        sqlx::query_scalar("SELECT uuid::text FROM agents.agents WHERE lower(name) = lower($1)")
            .bind(seeded_name)
            .fetch_one(dest)
            .await?;
    let source_uuid: String =
        sqlx::query_scalar("SELECT uuid::text FROM agents.agents WHERE lower(name) = lower($1)")
            .bind(seeded_name)
            .fetch_one(source)
            .await?;
    assert_ne!(
        source_uuid, dest_uuid_before,
        "test precondition: seeded agent uuids must diverge across DBs"
    );

    let new_agent_name = format!("resync-observed-{}", &source_uuid[..8]);
    let expected_personas = vec!["Offline resync persona".to_string()];
    let expected_name_variants = vec!["offline-resync-agent".to_string()];
    sqlx::query(
        "INSERT INTO agents.agents
            (name, description, persona_aliases, name_variants)
         VALUES ($1, 'observed offline', $2, $3)",
    )
    .bind(&new_agent_name)
    .bind(&expected_personas)
    .bind(&expected_name_variants)
    .execute(source)
    .await?;

    Ok(DivergedAgentFixture {
        seeded_name,
        dest_uuid_before,
        new_agent_name,
        expected_personas,
        expected_name_variants,
    })
}

async fn assert_seeded_agent_resolved(
    dest: &PgPool,
    fixture: &DivergedAgentFixture,
) -> anyhow::Result<()> {
    let seeded_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agents.agents WHERE lower(name) = lower($1)")
            .bind(fixture.seeded_name)
            .fetch_one(dest)
            .await?;
    assert_eq!(
        seeded_rows, 1,
        "seeded agent must not be duplicated on dest"
    );

    let dest_uuid_after: String =
        sqlx::query_scalar("SELECT uuid::text FROM agents.agents WHERE lower(name) = lower($1)")
            .bind(fixture.seeded_name)
            .fetch_one(dest)
            .await?;
    assert_eq!(
        dest_uuid_after, fixture.dest_uuid_before,
        "seeded agent's dest uuid must be unchanged (resolved, not re-inserted)"
    );

    let dup_names: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT lower(name) FROM agents.agents GROUP BY lower(name) HAVING count(*) > 1
         ) d",
    )
    .fetch_one(dest)
    .await?;
    assert_eq!(
        dup_names, 0,
        "no duplicate lower(name) rows may exist on dest"
    );

    Ok(())
}

async fn assert_new_agent_inserted(
    dest: &PgPool,
    fixture: &DivergedAgentFixture,
) -> anyhow::Result<()> {
    let new_present: i64 = sqlx::query_scalar("SELECT count(*) FROM agents.agents WHERE name = $1")
        .bind(&fixture.new_agent_name)
        .fetch_one(dest)
        .await?;
    assert_eq!(
        new_present, 1,
        "genuinely-new observed agent must insert on dest"
    );

    let (actual_personas, actual_name_variants): (Vec<String>, Vec<String>) = sqlx::query_as(
        "SELECT persona_aliases, name_variants
           FROM agents.agents
          WHERE name = $1",
    )
    .bind(&fixture.new_agent_name)
    .fetch_one(dest)
    .await?;
    assert_eq!(
        actual_personas, fixture.expected_personas,
        "resync must carry persona aliases on the owning agent row"
    );
    assert_eq!(
        actual_name_variants, fixture.expected_name_variants,
        "resync must carry name variants on the owning agent row"
    );

    Ok(())
}

#[tokio::test]
async fn replay_pending_resolves_seeded_agents_across_divergent_uuids() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine seeded-agent test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    baseline_cursors(&source, BASELINE_TABLES).await?;

    let fixture = diverge_dest_agents_and_seed_new_agent(&source, &dest).await?;
    let dest_agents_before = count(&dest, "agents.agents").await?;

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    let report = engine.replay_pending().await.map_err(|e| {
        anyhow::anyhow!("replay_pending must not violate uq_agents_lower_name: {e}")
    })?;
    eprintln!("seeded-agent replay report: {report:?}");

    let dest_agents_after = count(&dest, "agents.agents").await?;
    assert_eq!(
        dest_agents_after - dest_agents_before,
        1,
        "only the genuinely-new agent inserts; the seeded roster resolves"
    );

    assert_seeded_agent_resolved(&dest, &fixture).await?;
    assert_new_agent_inserted(&dest, &fixture).await?;

    let report2 = engine.replay_pending().await?;
    eprintln!("seeded-agent second replay report: {report2:?}");
    let dest_agents_final = count(&dest, "agents.agents").await?;
    assert_eq!(
        dest_agents_final, dest_agents_after,
        "second replay must not insert or duplicate agents"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}
