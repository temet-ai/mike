use bytes::Bytes;
use serde_json::{Value, json};

use super::super::samples::*;
use super::*;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_cache::LessonSnapshot;
use crate::store::models::RequestId;

struct Fixture {
    state: crate::state::AppState,
    toggles: ResolvedToggles,
    filter_cfg: FilterConfig,
    lesson_snapshot: LessonSnapshot,
    turn_origin: TurnOrigin,
    has_agent_id: bool,
}

impl Fixture {
    async fn new(session_id: &str, on: bool) -> anyhow::Result<Self> {
        let config = crate::config::ProxyConfig::for_tests_in_memory();
        let state = crate::state::AppState::new(config).await?;
        let mut toggles: ResolvedToggles = (*state.resolve_session_toggles(session_id)).clone();
        toggles.repeated_notices = on;
        Ok(Self {
            state,
            toggles,
            filter_cfg: FilterConfig::default(),
            lesson_snapshot: LessonSnapshot::default(),
            turn_origin: TurnOrigin::Primary,
            has_agent_id: false,
        })
    }

    fn ctx<'a>(&'a self, session_id: &'a str) -> StageContext<'a> {
        StageContext {
            state: &self.state,
            session_toggles: &self.toggles,
            filter_cfg: &self.filter_cfg,
            lesson_snapshot: &self.lesson_snapshot,
            session_id,
            project_path: "",
            turn_origin: &self.turn_origin,
            request_id: RequestId::next(),
            is_tool_loop: false,
            subagent_persona: None,
            has_agent_id: self.has_agent_id,
        }
    }

    async fn apply(&self, session_id: &str, request: &Value) -> anyhow::Result<StageBody> {
        let raw = serde_json::to_vec(request)?;
        let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
        RepeatedNoticesStage
            .apply(&self.ctx(session_id), &mut body)
            .await;
        Ok(body)
    }
}

fn reason(body: StageBody) -> String {
    let manifest = body.into_parts().manifest.finish("sent".to_string());
    manifest.stages[0].reason_code.clone()
}

fn mail_in_a_tool_result(bodies: &[&str], thread: Value) -> Value {
    json!({
        "messages": [{"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1",
             "content": tool_output_with_mail("ok", bodies)}
        ]}],
        "thread": thread
    })
}

fn continuing() -> Value {
    json!({"type": "continue", "previous_message_id": "msg_1"})
}

#[tokio::test]
async fn the_stage_runs_only_with_its_toggle_on_and_on_the_sessions_own_lane() -> anyhow::Result<()>
{
    let session = "repeated-notices-toggle";
    let off = Fixture::new(session, false).await?;
    assert!(!RepeatedNoticesStage.enabled(&off.ctx(session)));
    assert!(
        !off.state.resolve_session_toggles(session).repeated_notices,
        "a fresh install must forward the client's bytes"
    );
    let mut on = Fixture::new(session, true).await?;
    assert!(RepeatedNoticesStage.enabled(&on.ctx(session)));
    on.has_agent_id = true;
    assert!(!RepeatedNoticesStage.enabled(&on.ctx(session)));
    on.has_agent_id = false;
    on.turn_origin = TurnOrigin::Subagent;
    assert!(!RepeatedNoticesStage.enabled(&on.ctx(session)));
    Ok(())
}

#[tokio::test]
async fn a_repeated_notice_is_removed_from_the_forwarded_body_only() -> anyhow::Result<()> {
    let session = "repeated-notices-apply";
    let fixture = Fixture::new(session, true).await?;
    let request = json!({"messages": [
        {"role": "user", "content": pushed_while_idle("one")},
        {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
        {"role": "user", "content": pushed_while_idle("two")}
    ]});
    let body = fixture.apply(session, &request).await?;

    assert!(body.filter_applied());
    assert!(
        body.filtered_stored().is_none(),
        "the stored copy stays the client's bytes"
    );
    let sent = String::from_utf8(body.body().to_vec())?;
    assert_eq!(sent.matches("permission laundering").count(), 1);
    let manifest = body.into_parts().manifest.finish("sent".to_string());
    assert_eq!(manifest.stages[0].reason_code, "repeated_notices_removed");
    assert_eq!(
        manifest.stages[0].resolved_parameters["removed"]["peer_session"],
        json!(1)
    );
    Ok(())
}

#[tokio::test]
async fn a_session_that_sends_only_new_messages_gets_the_notice_once() -> anyhow::Result<()> {
    let session = "repeated-notices-continue";
    let fixture = Fixture::new(session, true).await?;

    let first = fixture
        .apply(
            session,
            &mail_in_a_tool_result(&["one", "two", "three"], continuing()),
        )
        .await?;
    let sent = String::from_utf8(first.body().to_vec())?;
    assert_eq!(sent.matches("NOT from your user").count(), 1);
    assert_eq!(reason(first), "repeated_notices_removed");

    let no_mail = json!({
        "messages": [{"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t2", "content": "ok"}
        ]}],
        "thread": continuing()
    });
    assert_eq!(
        reason(fixture.apply(session, &no_mail).await?),
        "repeated_notices_none"
    );

    let later = fixture
        .apply(session, &mail_in_a_tool_result(&["four"], continuing()))
        .await?;
    let sent = String::from_utf8(later.body().to_vec())?;
    assert!(sent.contains("four") && !sent.contains("NOT from your user"));
    assert_eq!(reason(later), "repeated_notices_removed");
    Ok(())
}

#[tokio::test]
async fn the_first_notice_is_forwarded_as_it_came() -> anyhow::Result<()> {
    let session = "repeated-notices-first";
    let fixture = Fixture::new(session, true).await?;
    let request = mail_in_a_tool_result(&["only"], continuing());
    let raw = serde_json::to_vec(&request)?;
    let body = fixture.apply(session, &request).await?;
    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    assert_eq!(reason(body), "repeated_notices_first_kept");
    Ok(())
}

#[tokio::test]
async fn a_new_conversation_without_the_notice_gets_it_again() -> anyhow::Result<()> {
    let session = "repeated-notices-new-thread";
    let fixture = Fixture::new(session, true).await?;
    fixture
        .apply(session, &mail_in_a_tool_result(&["one"], continuing()))
        .await?;

    let compacted = json!({
        "messages": [{"role": "user", "content": "summary of the work so far"}],
        "thread": {"type": "create"}
    });
    assert_eq!(
        reason(fixture.apply(session, &compacted).await?),
        "repeated_notices_none"
    );

    let after = fixture
        .apply(session, &mail_in_a_tool_result(&["two"], continuing()))
        .await?;
    assert!(!after.filter_applied());
    assert_eq!(reason(after), "repeated_notices_first_kept");
    Ok(())
}

#[tokio::test]
async fn a_request_that_only_quotes_the_wording_is_recorded_as_holding_no_notice()
-> anyhow::Result<()> {
    let session = "repeated-notices-quoted";
    let fixture = Fixture::new(session, true).await?;
    let request = json!({"messages": [
        {"role": "user", "content": format!("For instance\n\n{PEER}")},
        {"role": "assistant", "content": [{"type": "text", "text": CHANNEL}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": format!("{CHANNEL}\nrows: 1")}
        ]}
    ]});
    let body = fixture.apply(session, &request).await?;
    assert!(!body.filter_applied());
    assert_eq!(reason(body), "repeated_notices_none");
    Ok(())
}
