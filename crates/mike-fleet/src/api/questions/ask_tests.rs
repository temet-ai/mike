use axum::http::StatusCode;
use uuid::Uuid;

use super::{AskBody, prepare};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn body_text(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn ask_to_a_session_that_does_not_exist_is_a_404_naming_the_recipient() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (asker, missing) = (Uuid::new_v4(), Uuid::new_v4());
    persist
        .launch()
        .ensure_session_exists(&asker.to_string())
        .await?;
    let body = AskBody {
        from: asker.to_string(),
        to: missing.to_string(),
        question: "who owns this file?".to_owned(),
        blocked: true,
        about: None,
    };
    let Err(response) = prepare(&persist, &body).await else {
        anyhow::bail!("an ask to a missing session was accepted");
    };
    let (status, text) = body_text(*response).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{text}");
    assert!(text.contains(&missing.to_string()), "{text}");
    assert!(!text.contains("invariant"), "{text}");
    db.cleanup().await
}

#[tokio::test]
async fn ask_to_a_session_that_exists_is_prepared() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (asker, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    for session in [asker, recipient] {
        persist
            .launch()
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    let body = AskBody {
        from: asker.to_string(),
        to: recipient.to_string(),
        question: "who owns this file?".to_owned(),
        blocked: false,
        about: None,
    };
    let Ok(prepared) = prepare(&persist, &body).await else {
        anyhow::bail!("an ask to an existing session was refused");
    };
    assert_eq!(prepared.to.session, recipient);
    db.cleanup().await
}
