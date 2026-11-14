use serde_json::json;
use uuid::Uuid;

use super::fixtures_tests::Env;
use super::pass;

const CLOSE: &str = "close_orphan_launches";

async fn coordinator_mail(env: &Env, coordinator: i64) -> anyhow::Result<Vec<String>> {
    Ok(
        sqlx::query_scalar(
            "SELECT body FROM mailbox.messages WHERE to_session_id = $1 ORDER BY id",
        )
        .bind(coordinator)
        .fetch_all(env.pool())
        .await?,
    )
}

#[tokio::test]
async fn a_running_fleet_seat_is_left_alone_and_its_coordinator_told_once() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(CLOSE).await?;
    let coordinator = Uuid::new_v4();
    let coordinator_id = env.session(coordinator, "1 minute").await?;
    let run = format!("fleet-{}", Uuid::new_v4());
    env.running_fleet(&run, Some(coordinator)).await?;
    let (launch, seat) = env.orphan_launch(Some(&run)).await?;
    env.watch("orphan_launches").await?;

    assert_eq!(pass(&env.owners).await?.verified, 1);
    assert_eq!(env.launch_closed(launch).await?, None);
    let mail = coordinator_mail(&env, coordinator_id).await?;
    assert_eq!(mail.len(), 1);
    assert!(mail[0].contains(&seat.to_string()) && mail[0].contains(&run));
    let actions = env.actions(CLOSE).await?;
    assert_eq!(
        actions[0].effect.as_ref().map(|e| e["outcome"].clone()),
        Some(json!("told_coordinator"))
    );

    env.watch("orphan_launches").await?;
    env.watch("orphan_launches").await?;
    let later = pass(&env.owners).await?;
    assert_eq!((later.verified, later.measured.len()), (0, 0));
    assert_eq!(coordinator_mail(&env, coordinator_id).await?.len(), 1);
    assert_eq!(env.actions(CLOSE).await?[0].measured_at, None);
    env.cleanup().await
}

#[tokio::test]
async fn a_running_fleet_without_a_coordinator_is_refused_not_touched() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    env.only(CLOSE).await?;
    let run = format!("fleet-{}", Uuid::new_v4());
    env.running_fleet(&run, None).await?;
    let (launch, _) = env.orphan_launch(Some(&run)).await?;
    env.watch("orphan_launches").await?;
    assert_eq!(pass(&env.owners).await?.refused, 1);
    assert_eq!(env.launch_closed(launch).await?, None);
    let actions = env.actions(CLOSE).await?;
    assert!(
        actions[0]
            .refusal
            .as_deref()
            .is_some_and(|r| r.contains("no coordinator"))
    );
    env.cleanup().await
}
