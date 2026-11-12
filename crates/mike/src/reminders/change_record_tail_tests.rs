use std::process::Command;
use std::sync::Arc;

use bytes::Bytes;

use super::RemindersOutcome;
use super::change_record_tail::{
    ChangeRecordTailStage, TAIL_HEADING, tail_reminder, tail_reminder_text,
};
use super::client_context::prompt_context;
use super::stage_test_support::{StageCtxFixture, make_ctx};
use crate::api::changes::note_answer_tail;
use crate::change_record::ChangeKind;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};
use crate::store::models::RequestId;
use crate::store::persistence::change_record::test_seed::{self, Fixture};

fn user_body() -> Vec<u8> {
    serde_json::json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    })
    .to_string()
    .into_bytes()
}

async fn state_with_tail(on: bool) -> anyhow::Result<crate::state::AppState> {
    let state =
        crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.change_record_tail = on;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    Ok(state)
}

async fn enabled_for(on: bool, origin: TurnOrigin, is_tool_loop: bool) -> anyhow::Result<bool> {
    let state = state_with_tail(on).await?;
    let resolved: ResolvedToggles = (*state.resolve_session_toggles("tail-test")).clone();
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id: "tail-test",
        project_path: "proj",
        is_tool_loop,
        subagent_persona: None,
    });
    Ok(ChangeRecordTailStage.enabled(&ctx))
}

#[tokio::test]
async fn tail_stage_is_enabled_on_the_first_request_of_a_primary_turn_with_the_toggle_on()
-> anyhow::Result<()> {
    assert!(enabled_for(true, TurnOrigin::Primary, false).await?);
    assert!(enabled_for(true, TurnOrigin::PrimaryFirst, false).await?);
    Ok(())
}

#[tokio::test]
async fn tail_stage_is_disabled_when_the_toggle_is_off() -> anyhow::Result<()> {
    assert!(!enabled_for(false, TurnOrigin::Primary, false).await?);
    Ok(())
}

#[tokio::test]
async fn tail_stage_is_disabled_for_a_subagent_request() -> anyhow::Result<()> {
    assert!(!enabled_for(true, TurnOrigin::Subagent, false).await?);
    Ok(())
}

#[tokio::test]
async fn tail_stage_is_disabled_on_a_later_request_of_the_same_turn() -> anyhow::Result<()> {
    assert!(!enabled_for(true, TurnOrigin::Primary, true).await?);
    Ok(())
}

fn git(dir: &std::path::Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("git").arg("-C").arg(dir).args(args).status()?;
    anyhow::ensure!(status.success(), "git {args:?} failed in {dir:?}");
    Ok(())
}

struct Seat {
    f: Fixture,
    project: i64,
    other: i64,
    me: uuid::Uuid,
    me_id: i64,
    checkout: String,
    _dir: tempfile::TempDir,
}

async fn seat() -> anyhow::Result<Option<Seat>> {
    let Some(f) = test_seed::fixture().await? else {
        return Ok(None);
    };
    let dir = tempfile::tempdir()?;
    git(dir.path(), &["init", "-b", "main"])?;
    let checkout = dir.path().to_string_lossy().into_owned();
    let pool = f.handle.pool();
    let project = test_seed::project(pool).await?;
    let me_id = test_seed::session(pool, project, Some(&checkout)).await?;
    let other = test_seed::session(pool, project, None).await?;
    let me = f
        .handle
        .session_identities(&[me_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no identity"))?
        .uuid;
    Ok(Some(Seat {
        f,
        project,
        other,
        me,
        me_id,
        checkout,
        _dir: dir,
    }))
}

impl Seat {
    async fn edits(&self, session: i64, path: &str) -> anyhow::Result<()> {
        let mut event = test_seed::event(self.project, ChangeKind::Edit, &[path]);
        event.checkout = self.checkout.clone();
        event.session_id = Some(session);
        self.f.handle.insert_change_events(&[event]).await?;
        Ok(())
    }

    async fn other_edits(&self, path: &str) -> anyhow::Result<()> {
        self.edits(self.other, path).await
    }

    async fn i_edit(&self, path: &str) -> anyhow::Result<()> {
        self.edits(self.me_id, path).await
    }

    async fn reminder(&self) -> RemindersOutcome {
        tail_reminder(&self.f.handle, self.me, &user_body(), RequestId::next()).await
    }
}

#[tokio::test]
async fn tail_reminder_names_what_others_changed_once_and_then_nothing() -> anyhow::Result<()> {
    let Some(seat) = seat().await? else {
        return Ok(());
    };
    seat.i_edit("src/first.rs").await?;
    seat.i_edit("src/second.rs").await?;
    seat.other_edits("src/first.rs").await?;
    assert!(matches!(seat.reminder().await, RemindersOutcome::Unchanged));

    seat.other_edits("src/second.rs").await?;
    let RemindersOutcome::Appended {
        body,
        reminder_text,
        ..
    } = seat.reminder().await
    else {
        anyhow::bail!("expected the reminder after another seat's change");
    };
    assert!(reminder_text.starts_with(&format!("<system-reminder>\n{TAIL_HEADING}\n- ")));
    assert!(reminder_text.contains("src/second.rs"), "{reminder_text}");
    assert!(!reminder_text.contains("src/first.rs"), "{reminder_text}");
    let sent = String::from_utf8(body)?;
    eprintln!("request body carrying the reminder: {sent}");
    assert!(sent.contains(TAIL_HEADING));

    assert!(
        matches!(seat.reminder().await, RemindersOutcome::Unchanged),
        "the place moved, so the same lines are not shown twice"
    );
    seat.f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tail_reminder_is_absent_when_nothing_changed() -> anyhow::Result<()> {
    let Some(seat) = seat().await? else {
        return Ok(());
    };
    assert!(matches!(seat.reminder().await, RemindersOutcome::Unchanged));
    assert!(matches!(seat.reminder().await, RemindersOutcome::Unchanged));
    seat.f.db.cleanup().await?;
    Ok(())
}

#[test]
fn tail_reminder_text_lists_each_line_under_the_heading() {
    let text = tail_reminder_text(&["a edited x".to_owned(), "b wrote y".to_owned()]);
    assert_eq!(
        text,
        "<system-reminder>\nWhat others changed since you last looked:\n- a edited x\n- b wrote y\n</system-reminder>"
    );
}

async fn state_on_db(seat: &Seat, on: bool) -> anyhow::Result<crate::state::AppState> {
    let config = crate::config::ProxyConfig::try_parse_from([
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
        &seat.f.db.database_url,
    ])?;
    let state = crate::state::AppState::new(config).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.change_record_tail = on;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    Ok(state)
}

#[tokio::test]
async fn context_tail_advances_the_cursor_once_and_the_stage_stands_aside() -> anyhow::Result<()> {
    let Some(seat) = seat().await? else {
        return Ok(());
    };
    let state = state_on_db(&seat, true).await?;
    let me = seat.me.to_string();
    seat.i_edit("src/ctx.rs").await?;
    seat.other_edits("src/ctx.rs").await?;
    prompt_context(&state, seat.me, &me, &seat.checkout).await;

    seat.other_edits("src/ctx.rs").await?;
    let blocks = prompt_context(&state, seat.me, &me, &seat.checkout).await;
    let [block] = blocks.as_slice() else {
        anyhow::bail!("expected exactly the tail block: {blocks:?}");
    };
    assert_eq!(block.stage, "change_record_tail");
    assert!(block.text.contains("src/ctx.rs"), "{}", block.text);
    assert!(
        matches!(seat.reminder().await, RemindersOutcome::Unchanged),
        "the route moved the session's one cursor, so the same lines are not read again"
    );

    let resolved: ResolvedToggles = (*state.resolve_session_toggles(&me)).clone();
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id: &me,
        project_path: &seat.checkout,
        is_tool_loop: false,
        subagent_persona: None,
    });
    let mut body = StageBody::new(Bytes::from(user_body()), "orig".to_string());
    ChangeRecordTailStage.apply(&ctx, &mut body).await;
    assert!(!body.filter_applied());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(
        manifest.stages[0].reason_code,
        "change_record_tail_served_by_client"
    );
    state.shutdown().cancel();
    seat.f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn note_answer_carries_the_lines_with_the_toggle_on_and_nothing_with_it_off()
-> anyhow::Result<()> {
    let Some(seat) = seat().await? else {
        return Ok(());
    };
    seat.i_edit("src/noted.rs").await?;
    seat.i_edit("src/later.rs").await?;
    seat.other_edits("src/noted.rs").await?;
    let on = state_on_db(&seat, true).await?;
    assert_eq!(note_answer_tail(&on, seat.me).await, None);
    seat.other_edits("src/noted.rs").await?;
    let lines = note_answer_tail(&on, seat.me)
        .await
        .ok_or_else(|| anyhow::anyhow!("expected lines on the note's answer"))?;
    assert!(
        lines.iter().any(|l| l.contains("src/noted.rs")),
        "{lines:?}"
    );

    seat.other_edits("src/later.rs").await?;
    let off = state_on_db(&seat, false).await?;
    assert_eq!(note_answer_tail(&off, seat.me).await, None);
    on.shutdown().cancel();
    off.shutdown().cancel();
    seat.f.db.cleanup().await?;
    Ok(())
}
