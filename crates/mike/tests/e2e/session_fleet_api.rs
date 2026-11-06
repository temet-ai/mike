use crate::db_helpers;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;

async fn bootstrap_session_at(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let now = Utc::now();
    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $4)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;
    Ok(session_pk)
}

async fn bootstrap_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let now = Utc::now();
    sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)
}

async fn insert_mailbox_message(
    pool: &PgPool,
    to_session_pk: i64,
    from_session_pk: Option<i64>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
         VALUES (gen_random_uuid(), $1, $2, 'fleet-api test message')",
    )
    .bind(to_session_pk)
    .bind(from_session_pk)
    .execute(pool)
    .await?;
    Ok(())
}

async fn launch_as_fleet_seat(pool: &PgPool, session_id: &str) -> anyhow::Result<()> {
    let run = format!("fleet-api-{session_id}");
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(&run)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO launch.launches (session_uuid, backend, folder, run_id)
         VALUES ($1::text::uuid, 'herdr', '/work', $2)",
    )
    .bind(session_id)
    .bind(&run)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn session_with_peers_reports_in_fleet_with_correct_counts() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let now = Utc::now();
    let project_id = bootstrap_project(&pool, "test-fleet-api-1").await?;

    let session_id = "c0000000-0000-4000-8000-000000000001";
    let session_pk = bootstrap_session_at(&pool, project_id, session_id, now).await?;

    let peer1_id = "c0000000-0000-4000-8000-000000000002";
    let peer1_pk =
        bootstrap_session_at(&pool, project_id, peer1_id, now - Duration::hours(100)).await?;
    insert_mailbox_message(&pool, peer1_pk, Some(session_pk)).await?;

    let peer2_id = "c0000000-0000-4000-8000-000000000003";
    let peer2_pk = bootstrap_session_at(&pool, project_id, peer2_id, now).await?;
    insert_mailbox_message(&pool, session_pk, Some(peer2_pk)).await?;
    insert_mailbox_message(&pool, session_pk, Some(peer2_pk)).await?;

    insert_mailbox_message(&pool, session_pk, Some(session_pk)).await?;
    insert_mailbox_message(&pool, session_pk, None).await?;
    launch_as_fleet_seat(&pool, session_id).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/fleet"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(body["session_id"].as_str(), Some(session_id));
    assert_eq!(body["project_path"].as_str(), Some("test-fleet-api-1"));
    assert_eq!(body["in_fleet"].as_bool(), Some(true));
    assert_eq!(
        body["peer_count"].as_i64(),
        Some(2),
        "two distinct DIRECT peers (peer1 via outbound, peer2 via inbound); self-edge \
         and anonymous send must not inflate this"
    );
    assert_eq!(
        body["message_count"].as_i64(),
        Some(3),
        "1 outbound to peer1 + 2 inbound from peer2 = 3; self-edge and anonymous \
         send excluded"
    );
    assert_eq!(
        body["fleet_size"].as_i64(),
        Some(3),
        "the whole connected component: this session + peer1 + peer2"
    );
    let window_hours = body["window_hours"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("window_hours must be present"))?;
    assert!(
        window_hours >= 101,
        "window_hours ({window_hours}) must reach back past peer1's ~100h-old \
         last_seen so GET /api/fleet/graph would include it"
    );

    let resp = client
        .get(format!("{inspect_url}/api/sessions/{peer2_id}/fleet"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let peer: Value = resp.json().await?;
    assert_eq!(
        peer["in_fleet"].as_bool(),
        Some(false),
        "mail alone does not make a seat: this peer has no launch row with a run id"
    );
    assert_eq!(peer["fleet_size"].as_i64(), Some(3));

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn transitive_chain_reports_same_fleet_size_for_every_member() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let now = Utc::now();
    let project_id = bootstrap_project(&pool, "test-fleet-api-chain").await?;

    let a_id = "c0000000-0000-4000-8000-000000000010";
    let b_id = "c0000000-0000-4000-8000-000000000011";
    let c_id = "c0000000-0000-4000-8000-000000000012";

    let a_pk = bootstrap_session_at(&pool, project_id, a_id, now).await?;
    let b_pk = bootstrap_session_at(&pool, project_id, b_id, now).await?;
    let c_pk = bootstrap_session_at(&pool, project_id, c_id, now - Duration::hours(200)).await?;

    insert_mailbox_message(&pool, b_pk, Some(a_pk)).await?;
    insert_mailbox_message(&pool, c_pk, Some(b_pk)).await?;
    for id in [a_id, b_id, c_id] {
        launch_as_fleet_seat(&pool, id).await?;
    }

    let client = reqwest::Client::new();
    for (id, direct_peer_count) in [(a_id, 1), (b_id, 2), (c_id, 1)] {
        let resp = client
            .get(format!("{inspect_url}/api/sessions/{id}/fleet"))
            .send()
            .await?;
        assert_eq!(resp.status(), 200, "session {id}");
        let body: Value = resp.json().await?;
        assert_eq!(
            body["fleet_size"].as_i64(),
            Some(3),
            "session {id}: fleet_size must be the WHOLE 3-session component \
             regardless of how many DIRECT partners this member has"
        );
        assert_eq!(
            body["peer_count"].as_i64(),
            Some(direct_peer_count),
            "session {id}: peer_count stays the DIRECT-partner count, distinct from \
             fleet_size"
        );
        assert_eq!(body["in_fleet"].as_bool(), Some(true), "session {id}");
    }

    let resp = client
        .get(format!("{inspect_url}/api/sessions/{a_id}/fleet"))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    let window_hours = body["window_hours"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("window_hours must be present"))?;
    assert!(
        window_hours >= 201,
        "window_hours ({window_hours}) queried from A must cover C's ~200h-old \
         last_seen even though C is not A's direct peer"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_with_no_mailbox_traffic_reports_not_in_fleet() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let project_id = bootstrap_project(&pool, "test-fleet-api-2").await?;
    let session_id = "c0000000-0000-4000-8000-000000000004";
    bootstrap_session_at(&pool, project_id, session_id, Utc::now()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/fleet"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(body["in_fleet"].as_bool(), Some(false));
    assert_eq!(body["peer_count"].as_i64(), Some(0));
    assert_eq!(body["message_count"].as_i64(), Some(0));
    assert_eq!(
        body["fleet_size"].as_i64(),
        Some(1),
        "no fleet membership: the component is just this session itself"
    );
    assert_eq!(
        body["window_hours"].as_i64(),
        Some(24),
        "no fleet membership always reports the 24h floor"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_session_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/ffffffff-ffff-4fff-8fff-ffffffffffff/fleet"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
