use super::fixture;
use crate::launch::fleet::told;
use crate::store::persistence::fleet_repo::FleetPersist as _;

#[tokio::test]
async fn told_keys_survive_a_fresh_read_and_stay_inside_their_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = "told-run-a";
    assert!(!told::load(&f.handle, run).await.already("push-failed"));
    told::mark(
        &f.handle,
        run,
        &["push-failed".to_string(), "digest:1".to_string()],
    )
    .await;
    told::mark(&f.handle, run, &["push-failed".to_string()]).await;

    let after_restart = told::load(&f.handle, run).await;
    assert!(after_restart.already("push-failed"));
    assert!(after_restart.already("digest:1"));
    assert!(!after_restart.already("digest:2"));
    assert!(told::already(&f.handle, run, "digest:1").await);
    assert!(
        !told::load(&f.handle, "told-run-b")
            .await
            .already("push-failed")
    );
    assert_eq!(f.handle.fleet().told_keys(run).await?.len(), 2);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_introduction_is_claimed_once_and_can_be_released_when_the_mail_fails()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = "told-run-d";
    assert!(
        f.handle
            .fleet()
            .recorded_introductions(&[11, 12])
            .await?
            .is_empty()
    );
    assert_eq!(
        f.handle
            .fleet()
            .record_introductions(run, &[(11, 1), (12, 1)])
            .await?
            .len(),
        2
    );
    assert!(
        f.handle
            .fleet()
            .record_introductions(run, &[(11, 1)])
            .await?
            .is_empty()
    );
    let mut recorded = f
        .handle
        .fleet()
        .recorded_introductions(&[11, 12, 13])
        .await?;
    recorded.sort_unstable();
    assert_eq!(recorded, vec![11, 12]);
    f.handle.fleet().release_introduction(11).await?;
    assert_eq!(
        f.handle.fleet().recorded_introductions(&[11, 12]).await?,
        vec![12]
    );
    assert_eq!(
        f.handle
            .fleet()
            .record_introductions(run, &[(11, 1)])
            .await?,
        vec![11]
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn marking_nothing_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle.fleet().mark_told("told-run-c", &[]).await?;
    assert!(f.handle.fleet().told_keys("told-run-c").await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
