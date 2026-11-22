use std::time::Duration;

use sqlx::postgres::PgListener;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Machines {
    db: TestDbHandle,
    here: PersistHandle,
    other: PersistHandle,
    feed: ChangeFeed,
}

async fn machines() -> anyhow::Result<Option<Machines>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    here.attach_change_feed(feed.clone());
    Ok(Some(Machines {
        db,
        here,
        other,
        feed,
    }))
}

async fn mark_config(m: &Machines, id: i64, probe: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE steering.steerers SET config = config || jsonb_build_object('memo_probe', $2::bigint)
          WHERE id = $1",
    )
    .bind(id)
    .bind(probe)
    .execute(m.other.pool())
    .await?;
    Ok(())
}

fn probe(row: Option<&super::GuardSteererRow>) -> Option<i64> {
    row.and_then(|row| row.config.get("memo_probe"))
        .and_then(serde_json::Value::as_i64)
}

#[tokio::test]
async fn guard_steerer_answers_from_memory_until_the_steerers_table_moves() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let Some(first) = m.here.steerer().guard_steerer().await? else {
        anyhow::bail!("the migrations seed no turn-guard steerer");
    };
    mark_config(&m, first.id, 7).await?;
    let held = m.here.steerer().guard_steerer().await?;
    assert_eq!(
        held.as_ref(),
        Some(&first),
        "the change has not reached the feed"
    );
    let unfed = m.other.steerer().guard_steerer().await?;
    assert_eq!(
        probe(unfed.as_ref()),
        Some(7),
        "a handle with no feed reads the table"
    );
    m.feed.record(RowChangedTable::Steerers, None);
    let fresh = m.here.steerer().guard_steerer().await?;
    assert_eq!(probe(fresh.as_ref()), Some(7));
    m.db.cleanup().await
}

#[tokio::test]
async fn guard_steerer_memory_is_dropped_by_a_resync() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let Some(first) = m.here.steerer().guard_steerer().await? else {
        anyhow::bail!("the migrations seed no turn-guard steerer");
    };
    mark_config(&m, first.id, 9).await?;
    m.feed.resync();
    let fresh = m.here.steerer().guard_steerer().await?;
    assert_eq!(probe(fresh.as_ref()), Some(9));
    m.db.cleanup().await
}

#[tokio::test]
async fn steerers_trigger_notifies_a_changed_row_as_the_steerers_feed_table() -> anyhow::Result<()>
{
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let Some(guard) = m.here.steerer().guard_steerer().await? else {
        anyhow::bail!("the migrations seed no turn-guard steerer");
    };
    let mut listener = PgListener::connect(&m.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    mark_config(&m, guard.id, 3).await?;
    let wanted = guard.id.to_string();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut table = None;
    while let Ok(Ok(note)) = tokio::time::timeout_at(deadline, listener.recv()).await {
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["id"] == wanted && payload["table"] == "steerers" {
            table = Some(serde_json::from_value::<RowChangedTable>(
                payload["table"].clone(),
            )?);
            break;
        }
    }
    assert_eq!(table, Some(RowChangedTable::Steerers));
    m.db.cleanup().await
}
