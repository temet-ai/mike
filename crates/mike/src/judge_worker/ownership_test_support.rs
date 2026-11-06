use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::Utc;
use serde_json::json;

use crate::circuit_breaker::CircuitBreaker;
use crate::llm::limiter::CallLimiter;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_rejudge::{NewRejudgeRun, RejudgeWindow};
use crate::store::persistence::turn_facts::TurnFactsRow;
use crate::store::test_support::{
    TestDb, TestDbHandle, seed_project, seed_session_turns, seed_two_sessions,
};

use super::live_judge::{JudgeRun, LiveBundle, LiveJudge, judge_and_promote, load_live_bundle};
use super::live_shared::LiveJudgeShared;
use super::ownership_test_model::TestModel;
use super::turn_sweep::reserve_claim_batch;

pub(super) struct Fixture {
    pub(super) db: TestDbHandle,
    pub(super) persist: PersistHandle,
    pub(super) peer: PersistHandle,
    pub(super) bundle: LiveBundle,
    pub(super) turns: Vec<i64>,
    pub(super) model: TestModel,
    pub(super) llm: OpenRouterClient,
}

impl Fixture {
    pub(super) async fn new(turn_count: i64) -> anyhow::Result<Option<Self>> {
        let Some(test_db) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = test_db.acquire_isolated().await?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        let peer = PersistHandle::connect(&db.database_url).await?;
        let components = super::spawn_test_seams::default_v2_bundle_components("test-model");
        let bundle_id = persist
            .create_candidate_bundle("turn_judge", "", &components)
            .await?;
        persist
            .upsert_setting("turn_judge.live_bundle_id", &json!(bundle_id))
            .await?;
        let bundle = load_live_bundle(&persist, bundle_id)
            .await
            .context("test bundle")?;
        let turns = seed_turns(&persist, turn_count).await?;
        let model = TestModel::start().await?;
        let limiter = Arc::new(CallLimiter::new(
            100,
            Duration::from_secs(60),
            100,
            Duration::from_secs(60),
        ));
        let llm = OpenRouterClient::new_with_limiter(
            "sk-test".to_owned(),
            "test-model".to_owned(),
            Some(model.url()),
            Some(Duration::from_secs(90)),
            limiter,
        )?
        .with_role(LlmRole::JudgeWorker);
        Ok(Some(Self {
            db,
            persist,
            peer,
            bundle,
            turns,
            model,
            llm,
        }))
    }

    pub(super) fn judge<'a>(
        &'a self,
        persist: &'a PersistHandle,
        shared: &'a LiveJudgeShared,
    ) -> LiveJudge<'a> {
        LiveJudge {
            persist,
            llm: &self.llm,
            shared,
            bundle: &self.bundle,
            threshold: 0.5,
            session_judge_bundle_id: 0,
            backoff_base_secs: 60,
            backoff_max_secs: 600,
        }
    }

    pub(super) async fn judge_turns(
        &self,
        persist: &PersistHandle,
        turns: &[i64],
    ) -> anyhow::Result<JudgeRun> {
        let shared = LiveJudgeShared::default();
        let judge = self.judge(persist, &shared);
        let reservation = reserve_claim_batch(&self.llm, false, turns.len() as i64)
            .map_err(|delay| anyhow::anyhow!("test reservation deferred: {delay:?}"))?;
        Ok(judge_and_promote(&judge, &reservation, &mut breaker(), turns).await)
    }

    pub(super) async fn history(&self, persist: &PersistHandle) -> usize {
        let shared = LiveJudgeShared::default();
        super::rejudge::drain(&self.judge(persist, &shared), &mut breaker()).await
    }

    pub(super) async fn create_history(&self, cap: i32) -> anyhow::Result<i64> {
        Ok(self
            .persist
            .create_rejudge_run(&NewRejudgeRun {
                window: RejudgeWindow {
                    bundle_id: self.bundle.id,
                    since: Utc::now() - chrono::Duration::days(2),
                    until: Utc::now(),
                },
                daily_call_cap: cap,
                estimated_calls: self.turns.len() as i32,
                estimated_usd: None,
                created_by: "ownership-test",
                role: "turn_judge",
            })
            .await?)
    }

    pub(super) async fn verdict_count(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM memory.turn_verdicts WHERE bundle_id = $1"#,
            self.bundle.id,
        )
        .fetch_one(self.persist.pool())
        .await?)
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.model.stop().await?;
        self.persist.pool().close().await;
        self.peer.pool().close().await;
        self.db.cleanup().await
    }
}

pub(super) fn breaker() -> CircuitBreaker {
    CircuitBreaker::new(5, Duration::from_secs(1), Duration::from_secs(60))
}

async fn seed_turns(persist: &PersistHandle, count: i64) -> anyhow::Result<Vec<i64>> {
    let project = seed_project(persist.pool(), "judge-ownership").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(persist.pool(), &session, &other, project).await?;
    seed_session_turns(persist.pool(), &session, project, count).await?;
    let turns = sqlx::query!(
        "UPDATE memory.operator_turns SET closed_at = now() - interval '1 hour',
             created_at = now() - interval '2 hours', user_prompt = 'Please implement the retry',
             final_assistant_text = 'Implemented and tested the retry.'
         WHERE project_id = $1 RETURNING id, session_id, start_request_id",
        project,
    )
    .fetch_all(persist.pool())
    .await?;
    for turn in &turns {
        persist
            .upsert_turn_facts(&[facts(turn.id, turn.session_id, turn.start_request_id)])
            .await?;
    }
    let mut ids: Vec<_> = turns.into_iter().map(|turn| turn.id).collect();
    ids.sort_unstable();
    Ok(ids)
}

fn facts(turn: i64, session: i64, request: i64) -> TurnFactsRow {
    TurnFactsRow {
        operator_turn_id: turn,
        session_id: session,
        builder_version: "turn-facts-test".to_owned(),
        built_at: Utc::now(),
        max_request_id: request,
        prompt_kind: "genuine".to_owned(),
        prompt_rule: None,
        prompt_kind_at_close: "genuine".to_owned(),
        prompt_rule_at_close: None,
        next_state: "none_session_settled".to_owned(),
        next_genuine_request_id: None,
        next_genuine_turn_id: None,
        next_genuine_at: None,
        next_state_at_close: "none_session_settled".to_owned(),
        next_genuine_request_id_at_close: None,
        skipped_before_next: json!({}),
        reply_window_segment_ids: Vec::new(),
        reclassified_at: None,
        text_chars: 34,
        text_cut_at: 34,
        last_text_origin: None,
        text_spans: json!([]),
        stop_request_id: None,
        stop_reason: None,
        aborted: false,
        interrupted: false,
        close_cause: "next_prompt".to_owned(),
        checks_run: json!([]),
        checks_count: 0,
        checks_failed: 0,
        ended_on: "other".to_owned(),
        ended_on_version: "ended-on-v1".to_owned(),
        session_class: "operator".to_owned(),
        session_class_rule: "default".to_owned(),
    }
}
