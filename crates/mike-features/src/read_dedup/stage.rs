use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::read_dedup::DedupHit;
use crate::wire::manifest::AppliedStage;

pub struct ReadDedupStage;

#[async_trait::async_trait]
impl RequestStage for ReadDedupStage {
    fn name(&self) -> &'static str {
        "read_dedup"
    }

    fn enabled(&self, _ctx: &StageContext<'_>) -> bool {
        true
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let Ok(dedup_body) = serde_json::from_slice::<serde_json::Value>(body.body()) else {
            body.decline(
                "read_dedup",
                "read_dedup_body_not_json",
                serde_json::json!({}),
            );
            return;
        };
        let armed = ctx.session_toggles.compression_dedup_enabled;
        let (hits, rewritten) = if armed {
            ctx.state.store.write().await.rewrite_read_dedup(
                ctx.session_id,
                &dedup_body,
                chrono::Utc::now(),
            )
        } else {
            (
                ctx.state.store.write().await.observe_read_dedup(
                    ctx.session_id,
                    &dedup_body,
                    chrono::Utc::now(),
                ),
                None,
            )
        };

        for hit in &hits {
            tracing::debug!(
                request_id = %ctx.request_id,
                session_id = %ctx.session_id,
                file_path = %hit.file_path,
                prior_tool_use_id = %hit.prior_tool_use_id,
                prior_timestamp = %hit.prior_timestamp,
                byte_len = hit.byte_len,
                rewritten = rewritten.is_some(),
                "read-dedup: duplicate Read/cat content detected"
            );
        }

        let parameters = hit_parameters(armed, &hits);
        let Some(new_body) = rewritten else {
            body.decline("read_dedup", declined_reason(armed, hits.len()), parameters);
            return;
        };
        let Ok(new_bytes) = serde_json::to_vec(&new_body) else {
            body.decline("read_dedup", "read_dedup_serialize_failed", parameters);
            return;
        };
        body.rewrite_forwarded_only(
            new_bytes.clone().into(),
            AppliedStage {
                stage: "read_dedup",
                policy_id: "read_dedup",
                policy_version: 1,
                resolved_parameters: parameters,
                new_bytes: &new_bytes,
                targets: Vec::new(),
                reason_code: "duplicate_read",
                detail: None,
            },
        );
    }
}

fn hit_parameters(armed: bool, hits: &[DedupHit]) -> serde_json::Value {
    let would_save_bytes: u64 = hits.iter().map(|hit| hit.byte_len).sum();
    serde_json::json!({
        "armed": armed,
        "hit_count": hits.len(),
        "would_save_bytes": would_save_bytes,
    })
}

fn declined_reason(armed: bool, hit_count: usize) -> &'static str {
    match (hit_count, armed) {
        (0, _) => "read_dedup_no_duplicate",
        (_, false) => "read_dedup_observed_disarmed",
        (_, true) => "read_dedup_hits_not_rewritten",
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde_json::json;

    use super::*;
    use crate::extract::request::TurnOrigin;
    use crate::feature_toggles::ResolvedToggles;
    use crate::filter::FilterConfig;
    use crate::lesson_worker::LessonSnapshot;
    use crate::store::models::RequestId;

    fn make_ctx<'a>(
        state: &'a crate::state::AppState,
        session_toggles: &'a crate::feature_toggles::ResolvedToggles,
        filter_cfg: &'a FilterConfig,
        lesson_snapshot: &'a LessonSnapshot,
        turn_origin: &'a TurnOrigin,
        session_id: &'a str,
    ) -> StageContext<'a> {
        StageContext {
            state,
            session_toggles,
            filter_cfg,
            lesson_snapshot,
            session_id,
            project_path: "",
            turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: false,
        }
    }

    #[tokio::test]
    async fn read_dedup_stage_is_always_enabled() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-read-dedup-stage-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "session-read-dedup-stage-test",
        );
        assert!(ReadDedupStage.enabled(&ctx));
        Ok(())
    }

    #[tokio::test]
    async fn apply_never_mutates_the_forwarded_body() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let session_toggles = state.resolve_session_toggles("session-read-dedup-no-mutate-test");
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "session-read-dedup-no-mutate-test",
        );

        let raw = serde_json::to_vec(&json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
                ]}
            ]
        }))?;
        let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
        ReadDedupStage.apply(&ctx, &mut body).await;

        assert!(!body.filter_applied());
        assert!(body.filtered_stored().is_none());
        assert_eq!(body.body().as_ref(), raw.as_slice());
        let parts = body.into_parts();
        let manifest = parts.manifest.finish("orig".to_string());
        assert_eq!(manifest.stages.len(), 1);
        assert_eq!(manifest.stages[0].reason_code, "read_dedup_no_duplicate");
        Ok(())
    }

    #[tokio::test]
    async fn apply_rewrites_duplicate_read_with_stub_when_armed_by_default() -> anyhow::Result<()> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let resolved = state.resolve_session_toggles("session-read-dedup-hit-test");
        let mut session_toggles: ResolvedToggles = (*resolved).clone();
        session_toggles.compression_dedup_enabled = true;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "session-read-dedup-hit-test",
        );

        let first_read = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
                ]}
            ]
        });
        let mut first_body = StageBody::new(
            Bytes::from(serde_json::to_vec(&first_read)?),
            "orig".to_string(),
        );
        ReadDedupStage.apply(&ctx, &mut first_body).await;
        assert!(
            !first_body.filter_applied(),
            "a first-time Read is not a hit"
        );

        let second_read = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
                ]}
            ]
        });
        let mut second_body = StageBody::new(
            Bytes::from(serde_json::to_vec(&second_read)?),
            "orig".to_string(),
        );
        ReadDedupStage.apply(&ctx, &mut second_body).await;

        let stats = state
            .store
            .read()
            .await
            .dedup_stats("session-read-dedup-hit-test");
        assert_eq!(stats.hit_count, 1);
        assert!(
            second_body.filter_applied(),
            "an armed duplicate-content hit must rewrite the forwarded body"
        );
        assert!(
            second_body.filtered_stored().is_none(),
            "rewrite_forwarded_only must not stamp the stored audit copy, matching file_read_compression"
        );
        let rewritten: serde_json::Value = serde_json::from_slice(second_body.body())?;
        let stubbed = rewritten
            .pointer("/messages/3/content/0/content")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("expected the stubbed tool_result content"))?;
        assert!(
            stubbed.starts_with("[proxy: duplicate read suppressed"),
            "forwarded body must carry the G1 stub, got: {stubbed}"
        );
        assert!(
            stubbed.contains("t1"),
            "stub must cite the prior tool_use_id, got: {stubbed}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn apply_leaves_body_byte_identical_when_compression_dedup_disabled() -> anyhow::Result<()>
    {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let resolved = state.resolve_session_toggles("session-read-dedup-disarmed-test");
        let mut session_toggles: ResolvedToggles = (*resolved).clone();
        session_toggles.compression_dedup_enabled = false;
        let filter_cfg = FilterConfig::default();
        let lesson_snapshot = LessonSnapshot::default();
        let turn_origin = TurnOrigin::Primary;
        let ctx = make_ctx(
            &state,
            &session_toggles,
            &filter_cfg,
            &lesson_snapshot,
            &turn_origin,
            "session-read-dedup-disarmed-test",
        );

        let first_read = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
                ]}
            ]
        });
        let mut first_body = StageBody::new(
            Bytes::from(serde_json::to_vec(&first_read)?),
            "orig".to_string(),
        );
        ReadDedupStage.apply(&ctx, &mut first_body).await;

        let second_read = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
                ]}
            ]
        });
        let raw = serde_json::to_vec(&second_read)?;
        let mut second_body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
        ReadDedupStage.apply(&ctx, &mut second_body).await;

        let stats = state
            .store
            .read()
            .await
            .dedup_stats("session-read-dedup-disarmed-test");
        assert_eq!(
            stats.hit_count, 1,
            "measurement bookkeeping must still run when disarmed"
        );
        assert!(
            !second_body.filter_applied(),
            "compression_dedup_enabled=false must forward the body byte-identical"
        );
        assert_eq!(
            second_body.body().as_ref(),
            raw.as_slice(),
            "disarmed output must be byte-identical to the client's input"
        );
        Ok(())
    }
}
