use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use uuid::Uuid;

use crate::api::handlers::get_session_messages;
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn seed_session(handle: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    let project = format!("/tmp/poll-answers-{session}");
    sqlx::query(
        "WITH p AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, now(), now())
             RETURNING id
         )
         INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         SELECT $2, p.id, now(), now() FROM p",
    )
    .bind(&project)
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn send(handle: &PersistHandle, from: Uuid, to: Uuid) -> anyhow::Result<Uuid> {
    let message = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
         SELECT $1, t.id, f.id, 'poll answers'
           FROM sessions.sessions t, sessions.sessions f
          WHERE t.session_uuid = $2 AND f.session_uuid = $3",
    )
    .bind(message)
    .bind(to)
    .bind(from)
    .execute(handle.pool())
    .await?;
    Ok(message)
}

async fn poll(state: &AppState, session: Uuid, etag: Option<&str>) -> anyhow::Result<Response> {
    let mut headers = HeaderMap::new();
    if let Some(etag) = etag {
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(etag)?);
    }
    Ok(get_session_messages(State(state.clone()), Path(session), headers).await)
}

fn etag_of(response: &Response) -> anyhow::Result<String> {
    let value = response
        .headers()
        .get(header::ETAG)
        .ok_or_else(|| anyhow::anyhow!("no ETag on a {} answer", response.status()))?;
    Ok(value.to_str()?.to_owned())
}

async fn body_of(response: Response) -> anyhow::Result<String> {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok(String::from_utf8(bytes.to_vec())?)
}

#[tokio::test]
async fn session_messages_poll_answers_from_the_change_feed() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle.clone());
    state.poll_answers.freeze_clock(1_800_000_000);
    state.spawn_change_feed();
    let (own, other, peer) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for session in [own, other, peer] {
        seed_session(&handle, session).await?;
    }

    let first = poll(&state, own, None).await?;
    assert_eq!(first.status(), StatusCode::OK);
    let etag = etag_of(&first)?;

    let unchanged = poll(&state, own, Some(&etag)).await?;
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);

    send(&handle, peer, other).await?;
    state
        .change_feed
        .record(RowChangedTable::MailboxMessages, Some(&other.to_string()));
    let others_mail = poll(&state, own, Some(&etag)).await?;
    assert_eq!(others_mail.status(), StatusCode::NOT_MODIFIED);

    let message = send(&handle, peer, own).await?;
    state
        .change_feed
        .record(RowChangedTable::MailboxMessages, Some(&own.to_string()));
    let own_mail = poll(&state, own, Some(&etag)).await?;
    assert_eq!(own_mail.status(), StatusCode::OK);
    let etag = etag_of(&own_mail)?;
    assert!(body_of(own_mail).await?.contains(&message.to_string()));

    state.change_feed.resync();
    let resynced = poll(&state, own, Some(&etag)).await?;
    assert_eq!(resynced.status(), StatusCode::OK);
    let etag = etag_of(&resynced)?;

    handle.pool().close().await;
    let without_database = poll(&state, own, Some(&etag)).await?;
    assert_eq!(
        without_database.status(),
        StatusCode::NOT_MODIFIED,
        "an unchanged poll must not touch the database"
    );

    state.shutdown().cancel();
    db.cleanup().await
}
