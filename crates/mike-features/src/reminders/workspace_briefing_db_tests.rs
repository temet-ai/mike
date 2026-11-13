use crate::store::persistence::proxy_repo::ProxyPersist as _;
use std::time::{Duration, Instant};

use bytes::Bytes;
use uuid::Uuid;

use super::WorkspaceBriefingStage;
use crate::config::ProxyConfig;
use crate::extract::request::TurnOrigin;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};
use crate::proxy::engine::workspace_register::maybe_register_workspace_session;
use crate::reminders::stage_test_support::{StageCtxFixture, make_ctx};
use crate::state::AppState;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::workspace::{RegisterWorkspaceSession, WorkspaceRosterRow};
use crate::store::persistence::{PersistHandle, SENTINEL_PROJECT_PATH};
use crate::workspace::RosterScope;

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(None);
        };
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        crate::state::serve_every_stage_for_tests(&state);
        let persist = state
            .persist()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        Ok(Some(Self { db, state, persist }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }
}

async fn roster_rows_for_session(
    persist: &PersistHandle,
    project_path: &str,
    session_uuid: Uuid,
) -> anyhow::Result<Vec<WorkspaceRosterRow>> {
    let Some(project_id) = persist.find_project_id_by_path(project_path).await? else {
        return Ok(Vec::new());
    };
    let roster = persist
        .proxy()
        .fetch_workspace_roster(project_id, RosterScope::All)
        .await?;
    Ok(roster
        .into_iter()
        .filter(|row| row.session_uuid == session_uuid)
        .collect())
}

fn user_turn_body() -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::json!({
        "model": "claude-sonnet-4",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "hello"}]},
        ],
    }))?)
}

async fn apply_stage(
    state: &AppState,
    session_id: &str,
    project_path: &str,
) -> anyhow::Result<StageBody> {
    apply_stage_to(state, session_id, project_path, user_turn_body()?).await
}

async fn apply_stage_to(
    state: &AppState,
    session_id: &str,
    project_path: &str,
    raw: Vec<u8>,
) -> anyhow::Result<StageBody> {
    let resolved = state.resolve_session_toggles(session_id);
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id,
        project_path,
        is_tool_loop: false,
        subagent_persona: None,
    });
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    WorkspaceBriefingStage.apply(&ctx, &mut body).await;
    Ok(body)
}

#[tokio::test]
async fn first_primary_request_injects_briefing_then_second_is_suppressed() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let project_path = format!("/tmp/workspace-briefing-test-{}", Uuid::new_v4());
    let session_uuid = Uuid::new_v4();
    let session_id = session_uuid.to_string();

    let body = apply_stage(&env.state, &session_id, &project_path).await?;
    assert!(body.filter_applied());
    let body_text = String::from_utf8_lossy(body.body()).to_string();
    assert!(
        body_text.contains("proxy-workspace-briefing"),
        "the injected reminder must carry its source marker: {body_text}"
    );
    assert!(
        body_text.contains(&session_id),
        "the injected reminder must name this session's own id: {body_text}"
    );
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(
        manifest.stages[0].reason_code, "workspace_briefing_injected",
        "the first primary request must record the injection reason code"
    );

    let second = apply_stage(&env.state, &session_id, &project_path).await?;
    assert!(
        !second.filter_applied(),
        "a session already briefed in this process must not be briefed again"
    );
    let second_manifest = second.into_parts().manifest.finish("orig".to_string());
    assert_eq!(
        second_manifest.stages[0].reason_code, "workspace_briefing_already_delivered",
        "the second request for the same session must record the once-only reason code"
    );

    env.cleanup().await
}

#[tokio::test]
async fn briefing_names_a_peer_registered_on_the_same_project() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let project_path = format!("/tmp/workspace-briefing-peer-test-{}", Uuid::new_v4());
    let peer_uuid = Uuid::new_v4();
    env.persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: peer_uuid,
            project_path: Some(&project_path),
            cwd: &project_path,
            worktree: None,
            registered_via: "rest",
            intent: Some(("peer goal", "")),
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;

    let session_uuid = Uuid::new_v4();
    let session_id = session_uuid.to_string();
    env.persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid,
            project_path: Some(&project_path),
            cwd: &project_path,
            worktree: None,
            registered_via: "rest",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    let body = apply_stage(&env.state, &session_id, &project_path).await?;
    let body_text = String::from_utf8_lossy(body.body()).to_string();
    assert!(
        body_text.contains(&peer_uuid.to_string()),
        "the briefing must list the peer already registered on this project: {body_text}"
    );
    assert!(body_text.contains("peer goal"), "{body_text}");

    env.cleanup().await
}

#[tokio::test]
async fn context_briefing_equals_the_stage_text_and_the_stage_stands_aside() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let mut toggles = (**env.state.feature_toggle_overlay.load()).clone();
    toggles.workspace_briefing = true;
    env.state
        .feature_toggle_overlay
        .store(std::sync::Arc::new(toggles));

    let project_path = format!("/tmp/workspace-briefing-context-test-{}", Uuid::new_v4());
    let session_uuid = Uuid::new_v4();
    let session_id = session_uuid.to_string();

    let blocks = crate::reminders::client_context::prompt_context(
        &env.state,
        session_uuid,
        &session_id,
        &project_path,
    )
    .await;
    let (briefing, _) = super::render_workspace_briefing(&env.state, &session_id, &project_path)
        .await
        .map_err(|_| anyhow::anyhow!("the briefing must render for a fresh session"))?;
    let expected = crate::reminders::client_context::briefing_blocks(&briefing);
    assert_eq!(blocks, expected);
    let stage_text = expected
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let delivered = serde_json::to_vec(&serde_json::json!({
        "model": "claude-sonnet-4",
        "messages": [{"role": "user", "content": [{"type": "text", "text": stage_text}]}],
    }))?;
    let body = apply_stage_to(&env.state, &session_id, &project_path, delivered).await?;
    assert!(!body.filter_applied());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(
        manifest.stages[0].reason_code,
        "workspace_briefing_already_delivered"
    );

    let other = Uuid::new_v4().to_string();
    let untouched = apply_stage(&env.state, &other, &project_path).await?;
    assert!(
        untouched.filter_applied(),
        "a session that never asked for its context is still briefed by the proxy"
    );

    env.cleanup().await
}

#[tokio::test]
async fn stage_and_capture_path_share_one_registration() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let project_path = format!("/tmp/workspace-briefing-shared-test-{}", Uuid::new_v4());
    let session_uuid = Uuid::new_v4();
    let session_id = session_uuid.to_string();

    let body = apply_stage(&env.state, &session_id, &project_path).await?;
    assert!(body.filter_applied());

    maybe_register_workspace_session(&env.state, &session_id, &project_path, true);

    let start = Instant::now();
    loop {
        let rows =
            roster_rows_for_session(&env.persist, SENTINEL_PROJECT_PATH, session_uuid).await?;
        assert!(
            rows.len() <= 1,
            "the passive capture-path spawn must not register a second row \
             once the stage has already claimed this session's cell: found {} rows",
            rows.len()
        );
        if !rows.is_empty() {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!(
                "timed out after {:?} waiting for the stage-driven row to be visible",
                start.elapsed()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    env.cleanup().await
}

#[tokio::test]
async fn briefing_path_is_the_reading_machines_own_location_or_says_it_has_none()
-> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let stored = format!("/mnt/data/projects/briefing-path-{}", Uuid::new_v4());
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(&stored)
    .fetch_one(env.persist.pool())
    .await?;
    let (linux, mac, bare) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mac_root = format!("/Users/dawid/projects/briefing-path-{}", Uuid::new_v4());
    for (machine, name, root) in [
        (linux, "omit", stored.as_str()),
        (mac, "mac", mac_root.as_str()),
    ] {
        sqlx::query("INSERT INTO projects.machines (machine_uuid, name) VALUES ($1, $2)")
            .bind(machine)
            .bind(name)
            .execute(env.persist.pool())
            .await?;
        sqlx::query(
            "INSERT INTO projects.locations (project_id, machine_uuid, root_path) VALUES ($1, $2, $3)",
        )
        .bind(project_id)
        .bind(machine)
        .bind(root)
        .execute(env.persist.pool())
        .await?;
    }

    let on = |machine| super::path_on_machine(&env.persist, project_id, &stored, machine);
    assert_eq!(on(Some(linux)).await, stored);
    assert_eq!(on(Some(mac)).await, mac_root);
    assert_eq!(
        on(Some(bare)).await,
        super::NO_LOCATION_ON_THIS_MACHINE,
        "a machine with no location must say so, never show another machine's path"
    );
    assert_eq!(on(None).await, stored);

    env.cleanup().await
}
