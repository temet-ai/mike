use mike::store::persistence::PersistHandle;
use mike::store::persistence::compositions::{NewCompositionParams, PatchCompositionParams};
use mike::store::persistence::proxy_repo::ProxyPersist as _;
use mike::wire::composition::CompositionMode;

use crate::test_db::TestDb;

#[tokio::test]
async fn list_agents_with_evidence_counts_linked_compositions() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let blocks = serde_json::json!([
        {"text": "billing-header"},
        {"text": "identity preamble"},
        {"text": format!("counts-test-body-{}", uuid::Uuid::new_v4())},
    ]);
    let agent_name = format!("counts-agent-{}", uuid::Uuid::new_v4());
    let lonely_agent_name = format!("lonely-agent-{}", uuid::Uuid::new_v4());
    let sha_a = uuid::Uuid::new_v4().simple().to_string().repeat(2);
    let sha_c = format!("c{}", &sha_a[1..]);

    let comp_a = handle
        .proxy()
        .create_composition(NewCompositionParams {
            prompt_sha256: &sha_a,
            label: "linked-a",
            blocks: &blocks,
            mode: Some(CompositionMode::Observe),
            agent_name: Some(&agent_name),
            agent_definition_path: Some("/tmp/counts-agent.md"),
        })
        .await?;
    let linked_agent_id = comp_a
        .agent_id
        .ok_or_else(|| anyhow::anyhow!("linked composition agent_id missing"))?;

    let unlinked = handle
        .proxy()
        .create_composition(NewCompositionParams {
            prompt_sha256: &sha_c,
            label: "unlinked-comp",
            blocks: &blocks,
            mode: Some(CompositionMode::Observe),
            agent_name: None,
            agent_definition_path: None,
        })
        .await?;
    let repointed = handle
        .proxy()
        .patch_composition(
            unlinked.id,
            PatchCompositionParams {
                label: None,
                blocks: None,
                mode: None,
                notes: None,
                agent_name: Some(Some(&agent_name)),
                agent_definition_path: None,
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("patch_composition returned None for an existing row"))?;
    assert_eq!(
        repointed.agent_id,
        Some(linked_agent_id),
        "patching agent_name onto an existing agent must reuse that agent row, not mint a new one"
    );

    let lonely_agent_id: i64 = sqlx::query_scalar!(
        "INSERT INTO agents.agents (name) VALUES ($1) RETURNING id",
        lonely_agent_name,
    )
    .fetch_one(handle.pool())
    .await?;

    let agents = handle.list_agents_with_evidence().await?;
    let linked = agents
        .iter()
        .find(|a| a.agent.id == linked_agent_id)
        .ok_or_else(|| anyhow::anyhow!("linked agent missing from list"))?;
    assert_eq!(
        linked.composition_count, 2,
        "agent linked to two compositions must report composition_count = 2"
    );
    let lonely = agents
        .iter()
        .find(|a| a.agent.id == lonely_agent_id)
        .ok_or_else(|| anyhow::anyhow!("lonely agent missing from list"))?;
    assert_eq!(
        lonely.composition_count, 0,
        "agent with no compositions must COALESCE to composition_count = 0"
    );

    let comps = handle
        .proxy()
        .list_compositions_for_agent(linked_agent_id)
        .await?;
    assert_eq!(
        comps.len(),
        2,
        "list_compositions_for_agent must return both linked compositions"
    );
    for comp in &comps {
        assert_eq!(
            comp.agent_id,
            Some(linked_agent_id),
            "each returned composition must carry the agent bigint id via the FK"
        );
    }

    Ok(())
}

#[tokio::test]
async fn bulk_upsert_file_backed_agents_inserts_and_promotes() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let promote_name = format!("promote-me-{}", uuid::Uuid::new_v4());
    let bulk_a_name = format!("bulk-a-{}", uuid::Uuid::new_v4());
    let bulk_b_name = format!("bulk-b-{}", uuid::Uuid::new_v4());

    sqlx::query!(
        "INSERT INTO agents.agents (name, source_id)
             SELECT $1, s.id FROM agents.sources s WHERE s.code = 'observed'",
        promote_name,
    )
    .execute(handle.pool())
    .await?;

    let names = vec![bulk_a_name.clone(), bulk_b_name.clone()];
    let paths = vec!["/tmp/bulk-a.md".to_string(), "/tmp/bulk-b.md".to_string()];
    let descriptions = vec![Some("desc-a".to_string()), None];
    let inserted = handle
        .bulk_upsert_file_backed_agents(&names, &paths, &descriptions)
        .await?;
    assert_eq!(inserted, 2, "two new rows must be reported as upserted");

    #[derive(sqlx::FromRow)]
    struct AgentShape {
        source: String,
        definition_path: Option<String>,
    }
    let a = sqlx::query_as!(
        AgentShape,
        r#"SELECT s.code AS "source!", a.definition_path
                 FROM agents.agents a
                 JOIN agents.sources s ON s.id = a.source_id
                WHERE a.name = $1"#,
        bulk_a_name,
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(a.source, "file_backed");
    assert_eq!(a.definition_path.as_deref(), Some("/tmp/bulk-a.md"));

    let promote_names = vec![promote_name.clone()];
    let promote_paths = vec!["/tmp/promote-me.md".to_string()];
    let promote_descs = vec![Some("promoted desc".to_string())];
    let promoted = handle
        .bulk_upsert_file_backed_agents(&promote_names, &promote_paths, &promote_descs)
        .await?;
    assert_eq!(
        promoted, 1,
        "the promote upsert must report one affected row"
    );

    let p = sqlx::query_as!(
        AgentShape,
        r#"SELECT s.code AS "source!", a.definition_path
                 FROM agents.agents a
                 JOIN agents.sources s ON s.id = a.source_id
                WHERE a.name = $1"#,
        promote_name,
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        p.source, "file_backed",
        "observed source must be promoted to file_backed on conflict"
    );
    assert_eq!(
        p.definition_path.as_deref(),
        Some("/tmp/promote-me.md"),
        "blank definition_path must be filled via COALESCE on conflict"
    );

    Ok(())
}
