use bytes::Bytes;

use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::drain;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::wire::manifest::ManifestBuilder;

use super::support::{
    drain_fixture, reverted_row, seed_intervention, seed_request_row, user_turn_body,
};

fn advisory_reminder(intervention_uuid: uuid::Uuid) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Steerer {
            kind: "refocus".to_string(),
            intervention_uuid: intervention_uuid.to_string(),
            advisory: true,
        },
        payload: "<system-reminder>use the Monitor tool</system-reminder>".to_string(),
    }
}

fn mid_turn_body() -> Bytes {
    Bytes::from_static(
        br#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":[{"type":"text","text":"q"}]},{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"sleep 30"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}]}"#,
    )
}

async fn drained(
    state: &crate::config::AppState,
    session_uuid: &str,
    mut body: Bytes,
) -> Vec<SystemReminder> {
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    drain(
        state,
        session_uuid,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await
}

#[tokio::test]
async fn an_advisory_note_waits_while_the_session_is_in_the_middle_of_a_turn() -> anyhow::Result<()>
{
    let Some((db, handle, pool, state, _project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        return Ok(());
    };
    let future = chrono::Utc::now() + chrono::Duration::minutes(10);
    let intervention = seed_intervention(&handle, &pool, session_id, trigger_id, future).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, advisory_reminder(intervention));

    let injected = drained(&state, &session_uuid, mid_turn_body()).await;
    assert!(
        injected.is_empty(),
        "advice must not land in the middle of a tool loop"
    );
    let (reverted, _) = reverted_row(&pool, intervention).await?;
    assert!(!reverted, "a held note is not withdrawn");

    let injected = drained(&state, &session_uuid, user_turn_body()).await;
    assert_eq!(
        injected.len(),
        1,
        "the note is delivered with the next turn"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_advisory_note_survives_the_operators_next_message() -> anyhow::Result<()> {
    let Some((db, handle, pool, state, project_id, session_id, session_uuid, trigger_id)) =
        drain_fixture().await?
    else {
        return Ok(());
    };
    seed_request_row(&pool, project_id, session_id, "a brand new ask").await?;
    let future = chrono::Utc::now() + chrono::Duration::minutes(10);
    let intervention = seed_intervention(&handle, &pool, session_id, trigger_id, future).await?;
    state
        .store
        .write()
        .await
        .enqueue_reminder(&session_uuid, advisory_reminder(intervention));

    let injected = drained(&state, &session_uuid, user_turn_body()).await;
    assert_eq!(
        injected.len(),
        1,
        "advice about how to work still holds after the operator's next message"
    );
    let (reverted, _) = reverted_row(&pool, intervention).await?;
    assert!(!reverted);

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
