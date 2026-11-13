use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::fixture;
use super::seed::{micros_now, seed_launch, seed_run, seed_session};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::seed_project;

struct Pair {
    run: String,
    sender: Uuid,
    sender_id: i64,
    recipient: Uuid,
    recipient_launch: i64,
}

async fn pair(handle: &PersistHandle) -> anyhow::Result<Pair> {
    let run = format!("tripwire-{}", Uuid::new_v4());
    seed_run(handle, &run).await?;
    let project = seed_project(handle.pool(), &format!("/tmp/{run}")).await?;
    let sender = Uuid::new_v4();
    let recipient = Uuid::new_v4();
    let sender_id = seed_session(handle.pool(), project, sender).await?;
    seed_session(handle.pool(), project, recipient).await?;
    seed_launch(handle, &run, "sender", Some(sender), true).await?;
    let recipient_launch = closed_launch(handle, &run, "closed-worker", recipient).await?;
    Ok(Pair {
        run,
        sender,
        sender_id,
        recipient,
        recipient_launch,
    })
}

async fn closed_launch(
    handle: &PersistHandle,
    run: &str,
    seat: &str,
    session: Uuid,
) -> anyhow::Result<i64> {
    let id = seed_launch(handle, run, seat, Some(session), true).await?;
    handle
        .launch()
        .record_launch_closed(id, "complete", Some(true))
        .await?;
    Ok(id)
}

async fn attempts(
    handle: &PersistHandle,
    sender: Option<i64>,
    recipient: Uuid,
    failures: &[(&str, DateTime<Utc>)],
) -> anyhow::Result<Vec<i64>> {
    let reasons: Vec<_> = failures.iter().map(|(reason, _)| *reason).collect();
    let times: Vec<_> = failures.iter().map(|(_, time)| *time).collect();
    Ok(sqlx::query_scalar(
        "INSERT INTO mailbox.failed_sends
             (attempted_by_session_id, attempted_to, reason, attempted_at)
         SELECT $1, $2, failure.reason, failure.attempted_at
           FROM unnest($3::text[], $4::timestamptz[]) AS failure(reason, attempted_at)
         RETURNING id",
    )
    .bind(sender)
    .bind(recipient.to_string())
    .bind(&reasons)
    .bind(&times)
    .fetch_all(handle.pool())
    .await?)
}

async fn ids(handle: &PersistHandle, run: &str, base: DateTime<Utc>) -> anyhow::Result<Vec<i64>> {
    Ok(handle
        .fleet()
        .fleet_closed_mail_attempts(run, base - Duration::hours(1), base + Duration::hours(1))
        .await?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

#[tokio::test]
async fn closed_mail_reason_and_inclusive_time_window_return_only_matching_attempts()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pair = pair(&f.handle).await?;
    let since = micros_now()? + Duration::hours(1);
    let until = since + Duration::minutes(1);
    let inserted = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[
            ("recipient_ended", since),
            ("recipient_ended", until),
            ("recipient_ended", since - Duration::microseconds(1)),
            ("recipient_ended", until + Duration::microseconds(1)),
            ("database_error", since),
            ("strand_ttl_expired", since),
        ],
    )
    .await?;
    let rows = f
        .handle
        .fleet()
        .fleet_closed_mail_attempts(&pair.run, since, until)
        .await?;
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![inserted[1], inserted[0]]
    );
    assert_eq!(rows[0].attempted_at, until);
    assert_eq!(rows[1].attempted_at, since);
    for row in rows {
        assert_eq!(row.sender, pair.sender);
        assert_eq!(row.recipient, pair.recipient);
        assert_eq!(row.seat.as_deref(), Some("closed-worker"));
    }
    assert!(
        f.handle
            .fleet()
            .fleet_closed_mail_attempts(&pair.run, until, since)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_mail_attempt_before_authoritative_close_is_excluded_but_close_boundary_is_included()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pair = pair(&f.handle).await?;
    let closed_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT closed_at FROM launch.launches WHERE id = $1")
            .bind(pair.recipient_launch)
            .fetch_one(f.handle.pool())
            .await?;
    let inserted = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[
            ("recipient_ended", closed_at - Duration::microseconds(1)),
            ("recipient_ended", closed_at),
            ("recipient_ended", closed_at + Duration::microseconds(1)),
        ],
    )
    .await?;
    assert_eq!(
        ids(&f.handle, &pair.run, closed_at).await?,
        vec![inserted[2], inserted[1]]
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_mail_requires_both_sender_and_recipient_in_the_exact_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let own = pair(&f.handle).await?;
    let other = pair(&f.handle).await?;
    let base = micros_now()? + Duration::hours(1);
    let failure = [("recipient_ended", base)];
    let wanted = attempts(&f.handle, Some(own.sender_id), own.recipient, &failure).await?;
    attempts(&f.handle, Some(own.sender_id), other.recipient, &failure).await?;
    attempts(&f.handle, Some(other.sender_id), own.recipient, &failure).await?;
    let elsewhere = attempts(&f.handle, Some(other.sender_id), other.recipient, &failure).await?;
    attempts(&f.handle, None, own.recipient, &failure).await?;
    attempts(&f.handle, Some(own.sender_id), Uuid::new_v4(), &failure).await?;
    assert_eq!(ids(&f.handle, &own.run, base).await?, wanted);
    assert_eq!(ids(&f.handle, &other.run, base).await?, elsewhere);
    assert!(
        ids(&f.handle, &format!("{}-suffix", own.run), base)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_mail_latest_launch_reopen_or_other_run_excludes_historical_membership()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pair = pair(&f.handle).await?;
    let base = micros_now()? + Duration::hours(1);
    let wanted = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[("recipient_ended", base)],
    )
    .await?;
    assert_eq!(ids(&f.handle, &pair.run, base).await?, wanted);
    let reopened = seed_launch(
        &f.handle,
        &pair.run,
        "reopened-worker",
        Some(pair.recipient),
        true,
    )
    .await?;
    assert!(ids(&f.handle, &pair.run, base).await?.is_empty());
    f.handle
        .launch()
        .record_launch_closed(reopened, "complete", Some(true))
        .await?;
    assert_eq!(ids(&f.handle, &pair.run, base).await?, wanted);
    let rows = f
        .handle
        .fleet()
        .fleet_closed_mail_attempts(
            &pair.run,
            base - Duration::seconds(1),
            base + Duration::seconds(1),
        )
        .await?;
    assert_eq!(rows[0].seat.as_deref(), Some("reopened-worker"));
    let other_run = format!("{}-other", pair.run);
    seed_run(&f.handle, &other_run).await?;
    closed_launch(&f.handle, &other_run, "moved-recipient", pair.recipient).await?;
    assert!(ids(&f.handle, &pair.run, base).await?.is_empty());
    closed_launch(&f.handle, &pair.run, "returned-recipient", pair.recipient).await?;
    assert_eq!(ids(&f.handle, &pair.run, base).await?, wanted);
    seed_launch(
        &f.handle,
        &other_run,
        "moved-sender",
        Some(pair.sender),
        true,
    )
    .await?;
    assert!(ids(&f.handle, &pair.run, base).await?.is_empty());
    assert!(ids(&f.handle, &other_run, base).await?.is_empty());
    seed_launch(
        &f.handle,
        &pair.run,
        "returned-sender",
        Some(pair.sender),
        true,
    )
    .await?;
    assert_eq!(ids(&f.handle, &pair.run, base).await?, wanted);
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_mail_told_marker_survives_reconnect_and_excludes_only_its_run_and_attempt()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pair = pair(&f.handle).await?;
    let base = micros_now()? + Duration::hours(1);
    let inserted = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[("recipient_ended", base); 2],
    )
    .await?;
    let marker = format!("closed-mail:{}", inserted[1]);
    f.handle
        .fleet()
        .mark_told("unrelated-run", std::slice::from_ref(&marker))
        .await?;
    assert_eq!(
        ids(&f.handle, &pair.run, base).await?,
        vec![inserted[1], inserted[0]]
    );
    f.handle.fleet().mark_told(&pair.run, &[marker]).await?;
    let reconnected = PersistHandle::connect(&f.db.database_url).await?;
    assert_eq!(ids(&reconnected, &pair.run, base).await?, vec![inserted[0]]);
    reconnected.pool().close().await;
    f.db.cleanup().await
}

#[tokio::test]
async fn closed_mail_limit_keeps_newest_200_and_exposes_remaining_attempts_after_told()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pair = pair(&f.handle).await?;
    let base = micros_now()? + Duration::hours(1);
    let mut inserted = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[("recipient_ended", base); 205],
    )
    .await?;
    let older = attempts(
        &f.handle,
        Some(pair.sender_id),
        pair.recipient,
        &[("recipient_ended", base - Duration::seconds(1))],
    )
    .await?;
    inserted.sort_unstable_by(|left, right| right.cmp(left));
    let first = ids(&f.handle, &pair.run, base).await?;
    assert_eq!(first.len(), 200);
    assert_eq!(first, inserted[..200]);
    let marked: Vec<_> = first.iter().map(|id| format!("closed-mail:{id}")).collect();
    f.handle.fleet().mark_told(&pair.run, &marked).await?;
    let expected: Vec<_> = inserted[200..].iter().copied().chain(older).collect();
    assert_eq!(ids(&f.handle, &pair.run, base).await?, expected);
    f.db.cleanup().await
}
