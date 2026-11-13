use mike::store::persistence::PersistHandle;
use mike::store::persistence::proxy_repo::ProxyPersist as _;

use crate::test_db::TestDb;

struct SeededAnomalyFixture {
    request_id: i64,
    rule_a: uuid::Uuid,
    rule_b: uuid::Uuid,
}

async fn seed_structural_mismatch_anomaly_fixture(
    pool: &sqlx::PgPool,
) -> anyhow::Result<SeededAnomalyFixture> {
    let (project_id, session_fk) = seed_anomaly_project_and_session(pool).await?;
    let req_id = seed_anomaly_request(pool, session_fk, project_id).await?;
    let rsp_id = seed_structural_mismatch_system_prompt(pool, req_id).await?;
    let (rule_a, rule_b) = seed_two_firing_rules_on(pool, rsp_id).await?;

    Ok(SeededAnomalyFixture {
        request_id: req_id,
        rule_a,
        rule_b,
    })
}

async fn seed_anomaly_project_and_session(pool: &sqlx::PgPool) -> anyhow::Result<(i64, i64)> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
        format!("anomaly-proj-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(pool)
    .await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_fk: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         RETURNING id",
        session_id,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_fk))
}

async fn seed_anomaly_request(
    pool: &sqlx::PgPool,
    session_fk: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let req_id: i64 = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{\"thinking_enabled\": false}'::jsonb,
                '', '', rtt.id, rto.id, NOW()
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
        session_fk,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(req_id)
}

async fn seed_structural_mismatch_system_prompt(
    pool: &sqlx::PgPool,
    request_id: i64,
) -> anyhow::Result<i64> {
    let rsp_id: i64 = sqlx::query_scalar!(
        "INSERT INTO requests.system_prompts
            (request_id, prompt_sha256, override_id, system_prompt_id,
             override_applied, filter_modified, unknown_blocks)
         VALUES ($1, NULL, NULL, NULL, false, true, $2)
         RETURNING id",
        request_id,
        serde_json::json!({"reason": "test-structural-mismatch"}),
    )
    .fetch_one(pool)
    .await?;
    Ok(rsp_id)
}

async fn seed_two_firing_rules_on(
    pool: &sqlx::PgPool,
    request_system_prompt_id: i64,
) -> anyhow::Result<(uuid::Uuid, uuid::Uuid)> {
    let rule_a = uuid::Uuid::new_v4();
    let rule_b = uuid::Uuid::new_v4();
    let fr_a: i64 = sqlx::query_scalar!(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, enabled, source_id)
         SELECT $1, rk.id, $2, ra.id, true, rs.id
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'text_substring' AND ra.code = 'drop' AND rs.code = 'toml'
         RETURNING id",
        rule_a,
        format!("anomaly-rule-a-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(pool)
    .await?;
    let fr_b: i64 = sqlx::query_scalar!(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, enabled, source_id)
         SELECT $1, rk.id, $2, ra.id, true, rs.id
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'xml_tag' AND ra.code = 'drop' AND rs.code = 'toml'
         RETURNING id",
        rule_b,
        format!("anomaly-rule-b-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(pool)
    .await?;
    sqlx::query!(
        "INSERT INTO requests.system_prompt_firing_rules
            (request_system_prompt_id, filter_rule_id, ord)
         VALUES ($1, $2, 1), ($1, $3, 0)",
        request_system_prompt_id,
        fr_b,
        fr_a,
    )
    .execute(pool)
    .await?;
    Ok((rule_a, rule_b))
}

#[tokio::test]
async fn list_anomalies_reassembles_firing_rules_from_junction() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let fixture = seed_structural_mismatch_anomaly_fixture(handle.pool()).await?;

    let groups = handle
        .proxy()
        .list_anomalies_windowed(None, None, 100, 0)
        .await?;
    let group = groups
        .iter()
        .find(|g| {
            g.observations
                .iter()
                .any(|o| o.request_id == fixture.request_id)
        })
        .ok_or_else(|| {
            anyhow::anyhow!("structural-mismatch anomaly group for the seeded request must exist")
        })?;

    let mut got = group.firing_rule_ids.clone();
    got.sort();
    let mut want = vec![fixture.rule_a.to_string(), fixture.rule_b.to_string()];
    want.sort();
    assert_eq!(
        got, want,
        "list_anomalies must reassemble both fired rule uuids from the junction JOIN"
    );

    Ok(())
}
