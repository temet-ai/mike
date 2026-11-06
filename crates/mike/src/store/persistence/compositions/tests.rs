use super::*;
use crate::compose_prompt::PromptClassKey;
use crate::compose_prompt::decision::CompositionMode;
use crate::store::persistence::PersistHandle;

#[tokio::test]
async fn load_prompt_compositions_seeds_from_db() -> anyhow::Result<()> {
    use crate::store::persistence::test_support::acquire_ephemeral_db;

    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let blocks = serde_json::json!([
        {"text": "billing-header"},
        {"text": "identity preamble"},
        {"text": "load-test-body"},
    ]);

    let baseline = handle.load_prompt_compositions().await?.len();

    let linked_sha = "1".repeat(64);
    let linked = handle
        .create_composition(NewCompositionParams {
            prompt_sha256: &linked_sha,
            label: "linked-comp",
            blocks: &blocks,
            mode: Some(CompositionMode::Observe),
            agent_name: Some("load-test-agent"),
            agent_definition_path: Some("/tmp/load-test-agent.md"),
        })
        .await?;
    assert!(
        linked.agent_id.is_some(),
        "create_composition with agent_name must resolve and project the bigint agent_id FK"
    );
    let linked_agent_id = linked
        .agent_id
        .ok_or_else(|| anyhow::anyhow!("linked composition agent_id missing"))?;

    let unlinked_sha = "2".repeat(64);
    handle
        .create_composition(NewCompositionParams {
            prompt_sha256: &unlinked_sha,
            label: "unlinked-comp",
            blocks: &blocks,
            mode: Some(CompositionMode::Observe),
            agent_name: None,
            agent_definition_path: None,
        })
        .await?;

    let loaded = handle.load_prompt_compositions().await?;
    assert_eq!(
        loaded.len(),
        baseline + 2,
        "expected baseline + 2 compositions in the load map"
    );

    let linked_row = loaded
        .get(&PromptClassKey::Subagent("load-test-agent".to_string()))
        .ok_or_else(|| anyhow::anyhow!("linked composition missing from load map"))?;
    assert_eq!(linked_row.label, "linked-comp");
    assert_eq!(linked_row.agent_name.as_deref(), Some("load-test-agent"));
    assert_eq!(
        linked_row.agent_id,
        Some(linked_agent_id),
        "load_prompt_compositions must project pc.agent_id (bigint FK) directly"
    );

    let unlinked_row = loaded
        .get(&PromptClassKey::Main)
        .ok_or_else(|| anyhow::anyhow!("unlinked composition missing from load map"))?;
    assert_eq!(unlinked_row.label, "unlinked-comp");
    assert!(
        unlinked_row.agent_id.is_none(),
        "unlinked composition must carry agent_id = NULL"
    );

    db.cleanup().await?;
    Ok(())
}
