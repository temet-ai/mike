use std::time::Duration;

use serde_json::Value;
use sqlx::AssertSqlSafe;
use sqlx::postgres::PgListener;
use uuid::Uuid;

use crate::store::persistence::location_move::db_tests::Fixture;
use crate::store::persistence::test_support::seed_project;

struct Listening {
    f: Fixture,
    listener: PgListener,
}

impl Listening {
    async fn new() -> anyhow::Result<Self> {
        let f = Fixture::new().await?;
        let mut listener = PgListener::connect_with(f.handle.pool()).await?;
        listener.listen("proxy.row_changed").await?;
        Ok(Self { f, listener })
    }

    async fn exec(&self, sql: &str, location: Uuid) -> anyhow::Result<()> {
        sqlx::query(AssertSqlSafe(sql.to_owned()))
            .bind(location)
            .execute(self.f.handle.pool())
            .await?;
        Ok(())
    }

    async fn location_notices_until_marker(&mut self) -> anyhow::Result<Vec<Value>> {
        let marker = Uuid::new_v4();
        self.f.handle.ensure_machine(marker, "marker").await?;
        sqlx::query(
            "INSERT INTO projects.locations(project_id, machine_uuid, root_path)
             VALUES ($1, $2, '/test/marker')",
        )
        .bind(self.f.project)
        .bind(marker)
        .execute(self.f.handle.pool())
        .await?;
        let marker = marker.to_string();
        let mut seen = Vec::new();
        loop {
            let notification = tokio::time::timeout(Duration::from_secs(5), self.listener.recv())
                .await
                .map_err(|_| anyhow::anyhow!("timed out waiting for the marker notice"))??;
            let payload: Value = serde_json::from_str(notification.payload())?;
            if payload["table"] != "locations" {
                continue;
            }
            if payload["id"] == marker.as_str() {
                return Ok(seen);
            }
            seen.push(payload);
        }
    }
}

#[tokio::test]
async fn locations_presence_refresh_and_last_seen_bump_send_no_notice() -> anyhow::Result<()> {
    let mut l = Listening::new().await?;
    let location = l.f.location("/test/notice/a").await?;
    l.exec(
        "UPDATE projects.locations SET present_at = now() - interval '1 hour'
          WHERE location_uuid = $1",
        location,
    )
    .await?;
    let first = l.location_notices_until_marker().await?;
    assert_eq!(
        first.len(),
        2,
        "the insert and the first sighting: {first:?}"
    );
    assert_eq!(first[1]["id"], l.f.machine.to_string().as_str());

    l.exec(
        "UPDATE projects.locations SET present_at = now(), last_seen = now(), gone_at = NULL
          WHERE location_uuid = $1",
        location,
    )
    .await?;
    let refreshed = l.location_notices_until_marker().await?;
    assert!(refreshed.is_empty(), "a refresh is silent: {refreshed:?}");
    Ok(())
}

#[tokio::test]
async fn locations_root_move_and_project_merge_each_send_a_notice() -> anyhow::Result<()> {
    let mut l = Listening::new().await?;
    let location = l.f.location("/test/notice/b").await?;
    l.location_notices_until_marker().await?;

    l.exec(
        "UPDATE projects.locations SET root_path = '/test/notice/moved'
          WHERE location_uuid = $1",
        location,
    )
    .await?;
    let moved = l.location_notices_until_marker().await?;
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert_eq!(moved[0]["op"], "UPDATE");

    let target = seed_project(l.f.handle.pool(), "/test/notice/merge-target").await?;
    sqlx::query("UPDATE projects.projects SET merged_into_id = $2 WHERE id = $1")
        .bind(l.f.project)
        .bind(target)
        .execute(l.f.handle.pool())
        .await?;
    sqlx::query("UPDATE projects.projects SET merged_into_id = $2 WHERE id = $1")
        .bind(l.f.project)
        .bind(target)
        .execute(l.f.handle.pool())
        .await?;
    let merged = l.location_notices_until_marker().await?;
    assert_eq!(
        merged.len(),
        1,
        "the merge notifies, the repeat does not: {merged:?}"
    );
    assert_eq!(merged[0]["id"], "projects.projects");
    Ok(())
}
