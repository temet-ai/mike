use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use super::{MailClaimResponse, post_mail_claim};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, starve_pool};

struct Env {
    db: TestDbHandle,
    state: AppState,
    session: Uuid,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(None);
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project_id: i64 = sqlx::query_scalar(
            "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        )
        .bind(format!("proj-session-mail-{}", Uuid::new_v4()))
        .fetch_one(handle.pool())
        .await?;
        let session = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(session)
        .bind(project_id)
        .execute(handle.pool())
        .await?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        crate::feature_toggles::serve_every_stage_for_tests(&state);
        state.persist = Some(handle);
        Ok(Some(Self { db, state, session }))
    }

    async fn queue_mail(&self, body: &str) -> anyhow::Result<Uuid> {
        let message_uuid = Uuid::new_v4();
        let persist = self
            .state
            .persist()
            .ok_or_else(|| anyhow::anyhow!("persist"))?;
        persist
            .insert_message(message_uuid, self.session, None, body)
            .await?;
        self.state.store.write().await.enqueue_reminder(
            &self.session.to_string(),
            SystemReminder {
                payload: body.to_owned(),
                source: ReminderSource::SessionMailbox {
                    message_uuid,
                    from_session_uuid: None,
                },
            },
        );
        Ok(message_uuid)
    }

    async fn claim(&self) -> anyhow::Result<MailClaimResponse> {
        let Ok(Json(answer)) =
            post_mail_claim(State(self.state.clone()), Path(self.session.to_string())).await
        else {
            anyhow::bail!("a UUID session must be answered");
        };
        Ok(answer)
    }
}

#[tokio::test]
async fn mail_claim_non_uuid_session_is_400() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let answer = post_mail_claim(State(state), Path("not-a-uuid".to_owned())).await;
    assert_eq!(answer.err(), Some(StatusCode::BAD_REQUEST));
    Ok(())
}

#[tokio::test]
async fn mail_claim_answers_a_pending_message_once() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let message_uuid = env.queue_mail("please look at the failing test").await?;
    let first = env.claim().await?;
    let [block] = first.blocks.as_slice() else {
        anyhow::bail!("expected one block: {first:?}");
    };
    assert_eq!(block.source, "mailbox");
    assert_eq!(block.message_id, Some(message_uuid));
    assert!(block.text.contains("please look at the failing test"));
    assert!(env.claim().await?.blocks.is_empty());
    env.state.shutdown().cancel();
    env.db.cleanup().await
}

#[tokio::test]
async fn mail_claim_consumes_the_message_so_it_is_neither_redelivered_nor_dead_lettered()
-> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let message_uuid = env.queue_mail("claimed by the client").await?;
    let [_] = env.claim().await?.blocks.as_slice() else {
        anyhow::bail!("expected the queued message to be claimed");
    };
    let persist = env
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persist"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let (sent, consumed) = loop {
        let (sent, consumed): (bool, bool) = sqlx::query_as(
            "SELECT sent_at IS NOT NULL, consumed_at IS NOT NULL
               FROM mailbox.messages WHERE message_uuid = $1",
        )
        .bind(message_uuid)
        .fetch_one(persist.pool())
        .await?;
        if (sent && consumed) || Instant::now() >= deadline {
            break (sent, consumed);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(sent, "a message the client claimed must be stamped sent");
    assert!(
        consumed,
        "the client delivers it once, so mike must not redeliver or dead-letter it"
    );
    let redelivery = persist
        .bump_redelivery_attempt(message_uuid, 1, 1, 1)
        .await?;
    assert!(
        redelivery.is_none(),
        "a consumed message is not redelivered"
    );
    env.state.shutdown().cancel();
    env.db.cleanup().await
}

#[tokio::test]
async fn mail_claim_answers_a_launch_brief_once() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let persist = env
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persist"))?;
    let note = Uuid::new_v4();
    let mut new = crate::store::persistence::launch::NewLaunch::new("herdr", "/tmp/session-mail");
    new.session_uuid = Some(env.session);
    new.prompt_present = true;
    new.brief_mode = Some("note".to_owned());
    new.brief_text = Some("Fix defects #315.".to_owned());
    new.brief_note_uuid = Some(note);
    persist.insert_launch(&new).await?;
    let first = env.claim().await?;
    let [block] = first.blocks.as_slice() else {
        anyhow::bail!("expected one block: {first:?}");
    };
    assert_eq!(block.source, "launch_brief");
    assert_eq!(block.message_id, Some(note));
    assert!(block.text.contains("Fix defects #315."));
    assert!(env.claim().await?.blocks.is_empty());
    env.state.shutdown().cancel();
    env.db.cleanup().await
}

#[tokio::test]
async fn mail_claim_while_the_pool_is_stalled_answers_in_budget_and_keeps_the_message()
-> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let message_uuid = env.queue_mail("delivered after the stall").await?;
    let persist = env
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persist"))?
        .clone();
    let stall = starve_pool(persist.pool(), Duration::from_secs(3)).await?;
    let started = Instant::now();
    let during = env.claim().await?;
    let elapsed = started.elapsed();
    eprintln!("mail claim during a 3 s pool stall answered in {elapsed:?}");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    stall.await?;
    let mut delivered = during.blocks;
    let settle = Instant::now() + Duration::from_secs(5);
    while delivered.is_empty() && Instant::now() < settle {
        tokio::time::sleep(Duration::from_millis(50)).await;
        delivered = env.claim().await?.blocks;
    }
    let [block] = delivered.as_slice() else {
        anyhow::bail!("the message must survive the stall: {delivered:?}");
    };
    assert_eq!(block.message_id, Some(message_uuid));
    env.state.shutdown().cancel();
    env.db.cleanup().await
}
