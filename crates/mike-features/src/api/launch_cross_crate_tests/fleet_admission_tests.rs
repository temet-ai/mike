use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use serde_json::json;
use uuid::Uuid;

use crate::api::fleet::definition_patch::{PatchFleetRequest, save_definition};
use crate::api::fleet::definition_view::DefinitionError;
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::test_support::acquire_ephemeral_db;

#[tokio::test]
async fn definition_patch_stale_running_row_refuses_retry_after_stop_fence() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(persist.clone());
    let run = format!("stale-patch-{}", Uuid::new_v4());
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: None,
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: "/test/stale-definition".into(),
            definition: json!({"goal": "keep the stored goal"}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    seed_fleet_running(&persist, row.fleet_uuid, &run).await?;
    let stale = persist
        .fleet()
        .find_fleet(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing before stop"))?;
    persist
        .fleet()
        .set_fleet_state(row.fleet_uuid, "stopping", None, None)
        .await?;
    let fenced = persist
        .fleet()
        .find_fleet(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fleet missing after stop"))?;
    assert_ne!(stale.updated_at, fenced.updated_at);
    let result = save_definition(
        &state,
        &persist,
        stale,
        &PatchFleetRequest {
            patch: Some(json!({"goal": "must not replace the stored goal"})),
            ready: false,
        },
    )
    .await;
    assert!(matches!(result, Err(DefinitionError::Stopping(slug)) if slug == run));
    assert_eq!(persist.fleet().find_fleet(&run).await?, Some(fenced));
    db.cleanup().await
}
