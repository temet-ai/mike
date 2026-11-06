use chrono::Duration;
use mike::store::persistence::dossier_entries::EntryClose;

use super::entries;
use crate::turn_facts::fixture;
use crate::turn_facts::resolution::{build, message};

#[tokio::test]
async fn as_of_shows_open_before_the_close_was_recorded_and_terminal_after() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    message(&f, 9100, 0, "please commit when green", Some(100)).await?;
    message(&f, 9101, 100, "thanks", Some(200)).await?;
    build(&f).await?;
    let rows = entries(&f).await?;
    let entry = rows
        .first()
        .ok_or_else(|| anyhow::anyhow!("expected a directive entry"))?;
    sqlx::query("UPDATE memory.dossier_entries SET recorded_at = recorded_at - interval '3 hours' WHERE id = $1")
        .bind(entry.id)
        .execute(f.handle.pool())
        .await?;
    let close = EntryClose {
        id: entry.id,
        status: "withdrawn".to_owned(),
        closed_at: chrono::Utc::now(),
        closed_by: "requests.requests:9101".to_owned(),
    };
    f.handle.close_dossier_entries(&[close]).await?;
    let closed = entries(&f).await?;
    let close_recorded = closed[0]
        .close_recorded_at
        .ok_or_else(|| anyhow::anyhow!("close must be recorded"))?;
    let before = f
        .handle
        .list_entries_as_of(f.session_id, close_recorded - Duration::hours(1))
        .await?;
    assert_eq!(before[0].status, "open");
    assert!(before[0].closed_by.is_none());
    let after = f
        .handle
        .list_entries_as_of(f.session_id, close_recorded + Duration::seconds(1))
        .await?;
    assert_eq!(after[0].status, "withdrawn");
    let too_early = f
        .handle
        .list_entries_as_of(f.session_id, closed[0].recorded_at - Duration::seconds(1))
        .await?;
    assert!(too_early.is_empty(), "an entry recorded after T is absent");
    f.db.cleanup().await?;
    Ok(())
}
