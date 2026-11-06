use uuid::Uuid;

use mike::store::test_support::{seed_project, seed_session_turns, seed_two_sessions};

use crate::store::operator_turns::support::{shared_handle, unique};

#[tokio::test]
async fn get_open_turn_ref_returns_the_still_open_turn() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("w120-open-turn")).await?;
    let session = Uuid::new_v4().to_string();
    let spare = Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &session, &spare, project_id).await?;
    seed_session_turns(handle.pool(), &session, project_id, 1).await?;

    let (turn_id, request_id): (i64, i64) = sqlx::query_as(
        "UPDATE memory.operator_turns t
            SET closed_at = NULL
           FROM sessions.sessions s
          WHERE t.session_id = s.id AND s.session_uuid = $1::text::uuid
      RETURNING t.id, t.end_request_id",
    )
    .bind(&session)
    .fetch_one(handle.pool())
    .await?;

    let session_uuid: Uuid = session.parse()?;
    let found = handle.get_open_turn_ref(session_uuid).await?;
    assert_eq!(
        found,
        Some((turn_id, request_id)),
        "must return the reopened turn's id and its (only) request as end_request_id"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn get_open_turn_ref_none_when_every_turn_is_closed() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), &unique("w120-no-open-turn")).await?;
    let session = Uuid::new_v4().to_string();
    let spare = Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &session, &spare, project_id).await?;
    seed_session_turns(handle.pool(), &session, project_id, 1).await?;

    let session_uuid: Uuid = session.parse()?;
    let found = handle.get_open_turn_ref(session_uuid).await?;
    assert_eq!(
        found, None,
        "seed_session_turns closes every turn it creates — nothing should be open"
    );

    handle.pool().close().await;
    Ok(())
}
