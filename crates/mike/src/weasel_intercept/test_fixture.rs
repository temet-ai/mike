use std::sync::Arc;

use crate::config::{AppState, ProxyConfig};
use crate::feature_toggles::FeatureToggles;
use crate::settings::SettingsOverlay;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_captured_request, seed_project,
};

pub(crate) struct Fixture {
    pub(crate) db: TestDbHandle,
    pub(crate) state: AppState,
    pub(crate) session_uuid: String,
    pub(crate) request_id: i64,
}

pub(crate) async fn fixture() -> anyhow::Result<Option<Fixture>> {
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
    let persist = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on for the evidence tests"))?;
    let project_id = seed_project(persist.pool(), "weasel-evidence").await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let request_id = seed_captured_request(
        persist.pool(),
        &session_uuid,
        project_id,
        "primary",
        b"original request",
    )
    .await?;
    Ok(Some(Fixture {
        db,
        state,
        session_uuid,
        request_id,
    }))
}

pub(crate) struct EvidenceRow {
    pub(crate) request_id: Option<i64>,
    pub(crate) outcome: String,
    pub(crate) subject: Option<String>,
}

pub(crate) async fn rows_for(fx: &Fixture) -> anyhow::Result<Vec<EvidenceRow>> {
    let persist = fx
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    let rows = sqlx::query_as!(
        EvidenceRow,
        "SELECT ri.request_id, ri.outcome, ri.subject
           FROM requests.response_intercepts ri
           JOIN sessions.sessions s ON s.id = ri.session_id
          WHERE s.session_uuid = $1::text::uuid
          ORDER BY ri.id",
        &fx.session_uuid,
    )
    .fetch_all(persist.pool())
    .await?;
    Ok(rows)
}

pub(crate) fn armed_toggles(state: &AppState, words: &[&str]) {
    state.feature_toggle_overlay.store(Arc::new(FeatureToggles {
        weasel_intercept: true,
        ..FeatureToggles::default()
    }));
    state.settings_overlay.store(Arc::new(SettingsOverlay {
        weasel_intercept_forbidden_words: words.iter().map(|w| (*w).to_owned()).collect(),
        ..SettingsOverlay::default()
    }));
}

pub(crate) async fn drain_spawned_writes(state: &AppState) {
    state.tracker.close();
    state.tracker.wait().await;
}
