use mike::dossier::worker::build_for_claimed_turns;
use mike::store::persistence::dossier_repo::DossierPersist as _;

use super::resolution::{facts, message};
use super::{enable_builder, fixture};

#[tokio::test]
async fn setting_value_returns_stored_json_or_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(
        f.handle.setting_value("dossier.batch_sessions").await?,
        None
    );
    f.handle
        .upsert_setting("dossier.batch_sessions", &serde_json::json!(7))
        .await?;
    assert_eq!(
        f.handle.setting_value("dossier.batch_sessions").await?,
        Some(serde_json::json!(7))
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_backstop_builds_facts_for_claimed_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    assert!(
        build_for_claimed_turns(&f.handle, &[turn]).await?.inserted == 0,
        "the builder is off by default"
    );
    enable_builder(&f).await?;
    let report = build_for_claimed_turns(&f.handle, &[turn]).await?;
    assert_eq!(report.inserted, 1);
    assert_eq!(facts(&f).await?.len(), 1);
    assert_eq!(build_for_claimed_turns(&f.handle, &[]).await?.inserted, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn claimed_turn_build_is_skipped_while_the_toggle_is_off() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    assert!(!f.handle.dossier().dossier_builder_enabled().await?);
    let report = build_for_claimed_turns(&f.handle, &[turn]).await?;
    assert_eq!(report.inserted, 0);
    assert!(facts(&f).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_for_turns_lists_each_session_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = message(
        &f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    let second = message(&f, 9101, 100, "and then add the docs", Some(200)).await?;
    assert_eq!(
        f.handle
            .dossier()
            .sessions_for_turns(&[first, second])
            .await?,
        vec![f.session_id]
    );
    assert!(f.handle.dossier().sessions_for_turns(&[]).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
