use axum::http::StatusCode;
use uuid::Uuid;

use super::excluded;

#[test]
fn excluded_without_a_value_excludes_nobody() -> anyhow::Result<()> {
    let Ok(none) = excluded(None) else {
        anyhow::bail!("an absent exclude was refused");
    };
    assert_eq!(none, None);
    Ok(())
}

#[test]
fn excluded_with_a_session_uuid_names_it() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let Ok(named) = excluded(Some(&session.to_string())) else {
        anyhow::bail!("a session uuid was refused");
    };
    assert_eq!(named, Some(session));
    Ok(())
}

#[tokio::test]
async fn excluded_with_a_malformed_uuid_is_refused_naming_the_input() -> anyhow::Result<()> {
    let Err(refused) = excluded(Some("not-a-uuid")) else {
        anyhow::bail!("a malformed exclude was accepted");
    };
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(refused.into_body(), usize::MAX).await?;
    let text = String::from_utf8(body.to_vec())?;
    assert!(text.contains("invalid_exclude"), "{text}");
    assert!(text.contains("`not-a-uuid`"), "{text}");
    Ok(())
}
