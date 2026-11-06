use crate::seed;
use crate::test_db;

use super::fixtures::{SeedDeltaRowParams, seed_delta_row, seed_message, seed_request_row};
use chrono::{TimeZone, Utc};
use seed::{seed_project, seed_session};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

type DeltaRow = (
    i64,
    Option<i64>,
    String,
    i32,
    i64,
    String,
    String,
    i32,
    serde_json::Value,
    Option<String>,
);

struct RoundTripFixture {
    predecessor_id: i64,
    request_id: i64,
    added_id: i64,
    removed_id: i64,
    added_sha: String,
    removed_sha: String,
    added_body: serde_json::Value,
    removed_body: serde_json::Value,
}

async fn seed_round_trip_fixture(pool: &PgPool) -> anyhow::Result<RoundTripFixture> {
    let project_id = seed_project(
        pool,
        &format!("test-project-w2.8-roundtrip-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    seed_session(pool, project_id, &session_id).await?;
    let pred_created_at = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
    let cur_created_at = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 1).unwrap();
    let predecessor_id = seed_request_row(pool, project_id, &session_id, pred_created_at).await?;
    let request_id = seed_request_row(pool, project_id, &session_id, cur_created_at).await?;

    let added_sha = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let removed_sha = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let added_body = serde_json::json!({"role": "user", "content": "added msg"});
    let removed_body = serde_json::json!({"role": "user", "content": "removed msg"});
    let added_id = seed_message(pool, &added_sha, "user", added_body.clone()).await?;
    let removed_id = seed_message(pool, &removed_sha, "user", removed_body.clone()).await?;

    seed_delta_row(
        pool,
        SeedDeltaRowParams {
            request_id,
            predecessor_request_id: Some(predecessor_id),
            session_id: &session_id,
            change_kind: "added",
            ord: 2,
            message_id: added_id,
            message_sha256: &added_sha,
            removed_reason: None,
        },
    )
    .await?;
    seed_delta_row(
        pool,
        SeedDeltaRowParams {
            request_id,
            predecessor_request_id: Some(predecessor_id),
            session_id: &session_id,
            change_kind: "removed",
            ord: 0,
            message_id: removed_id,
            message_sha256: &removed_sha,
            removed_reason: Some("compaction"),
        },
    )
    .await?;

    Ok(RoundTripFixture {
        predecessor_id,
        request_id,
        added_id,
        removed_id,
        added_sha,
        removed_sha,
        added_body,
        removed_body,
    })
}

async fn fetch_delta_rows_for_request(
    pool: &PgPool,
    request_id: i64,
) -> anyhow::Result<Vec<DeltaRow>> {
    let rows = sqlx::query_as(
        "SELECT rd.request_id, rd.predecessor_request_id, ck.code, rd.ord,
                rd.message_id, m.message_sha256, ro.code, m.byte_size, m.body,
                rr.code
           FROM requests.deltas rd
           JOIN messages.messages m ON m.id = rd.message_id
           JOIN requests.delta_change_kinds ck ON ck.id = rd.change_kind_id
           JOIN messages.roles ro ON ro.id = m.role_id
           LEFT JOIN requests.delta_removed_reasons rr ON rr.id = rd.removed_reason_id
          WHERE rd.request_id = $1
          ORDER BY ck.code ASC, rd.ord ASC",
    )
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn assert_added_delta_row(row: &DeltaRow, fixture: &RoundTripFixture) {
    let (
        added_request_id,
        added_pred,
        added_kind,
        added_ord,
        added_message_id,
        added_sha_back,
        added_role,
        added_byte_size,
        added_body_back,
        added_reason,
    ) = row;
    assert_eq!(*added_request_id, fixture.request_id);
    assert_eq!(*added_pred, Some(fixture.predecessor_id));
    assert_eq!(added_kind, "added");
    assert_eq!(*added_ord, 2);
    assert_eq!(*added_message_id, fixture.added_id);
    assert_eq!(added_sha_back, &fixture.added_sha);
    assert_eq!(added_role, "user");
    assert_eq!(
        *added_byte_size,
        fixture.added_body.to_string().len() as i32
    );
    assert_eq!(added_body_back, &fixture.added_body);
    assert!(added_reason.is_none(), "added rows have no removed_reason");
}

fn assert_removed_delta_row(row: &DeltaRow, fixture: &RoundTripFixture) {
    let (
        removed_request_id,
        removed_pred,
        removed_kind,
        removed_ord,
        removed_message_id_back,
        removed_sha_back,
        removed_role,
        removed_byte_size,
        removed_body_back,
        removed_reason,
    ) = row;
    assert_eq!(*removed_request_id, fixture.request_id);
    assert_eq!(*removed_pred, Some(fixture.predecessor_id));
    assert_eq!(removed_kind, "removed");
    assert_eq!(*removed_ord, 0);
    assert_eq!(*removed_message_id_back, fixture.removed_id);
    assert_eq!(removed_sha_back, &fixture.removed_sha);
    assert_eq!(removed_role, "user");
    assert_eq!(
        *removed_byte_size,
        fixture.removed_body.to_string().len() as i32
    );
    assert_eq!(removed_body_back, &fixture.removed_body);
    assert_eq!(
        removed_reason.as_deref(),
        Some("compaction"),
        "removed rows MUST carry a reason"
    );
}

#[tokio::test]
async fn request_deltas_round_trip_added_and_removed_rows() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let fixture = seed_round_trip_fixture(&pool).await?;

    let rows = fetch_delta_rows_for_request(&pool, fixture.request_id).await?;
    assert_eq!(
        rows.len(),
        2,
        "expected 2 delta rows for request {}",
        fixture.request_id
    );

    assert_added_delta_row(&rows[0], &fixture);
    assert_removed_delta_row(&rows[1], &fixture);

    test_db.cleanup().await?;
    Ok(())
}
