use crate::db_helpers;
use crate::db_helpers::ProxyWithDb;
use serde_json::Value;

use super::support::{fire_request_and_get_id, mount_minimal_upstream, seed_intervention};

async fn seed_project_scoped_intervention(
    env: &ProxyWithDb,
    client: &reqwest::Client,
    session_uuid: &str,
    project_path: &str,
    prompt: &str,
    anchor_text: &str,
) -> anyhow::Result<i64> {
    let trigger_request_id =
        fire_request_and_get_id(client, &env.proxy_url, &env.pool, session_uuid, prompt).await?;

    sqlx::query(
        "WITH proj AS (
             INSERT INTO projects.projects (project_path, display_name)
             VALUES ($2, $2)
             RETURNING id
         )
         UPDATE sessions.sessions s
            SET project_id = (SELECT id FROM proj)
          WHERE s.session_uuid = $1::uuid",
    )
    .bind(session_uuid)
    .bind(project_path)
    .execute(&env.pool)
    .await?;

    seed_intervention(&env.pool, session_uuid, trigger_request_id, anchor_text).await?;
    Ok(trigger_request_id)
}

async fn assert_project_filter_isolates_matching_row(
    client: &reqwest::Client,
    inspect_url: &str,
    project_a: &str,
    project_b: &str,
) -> anyhow::Result<()> {
    let filtered_resp = client
        .get(format!(
            "{inspect_url}/api/steering/verifier-rate?project={project_a}"
        ))
        .send()
        .await?;
    assert_eq!(
        filtered_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for filtered verifier-rate; got {}",
        filtered_resp.status()
    );
    let filtered_body: Value = filtered_resp.json().await?;
    let filtered_rows = filtered_body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows must be an array; got {filtered_body}"));
    assert!(
        filtered_rows.iter().all(|r| r["project_path"] == project_a),
        "project={project_a} filter must return only that project's row; got {filtered_body}"
    );
    assert!(
        filtered_rows.iter().any(|r| r["project_path"] == project_a),
        "project={project_a} filter must surface the seeded row; got {filtered_body}"
    );

    let unfiltered_resp = client
        .get(format!("{inspect_url}/api/steering/verifier-rate"))
        .send()
        .await?;
    assert_eq!(
        unfiltered_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for unfiltered verifier-rate; got {}",
        unfiltered_resp.status()
    );
    let unfiltered_body: Value = unfiltered_resp.json().await?;
    let unfiltered_rows = unfiltered_body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows must be an array; got {unfiltered_body}"));
    assert!(
        unfiltered_rows
            .iter()
            .any(|r| r["project_path"] == project_a),
        "unfiltered call must still surface project-a's row; got {unfiltered_body}"
    );
    assert!(
        unfiltered_rows
            .iter()
            .any(|r| r["project_path"] == project_b),
        "unfiltered call must still surface project-b's row; got {unfiltered_body}"
    );
    Ok(())
}

#[tokio::test]
async fn get_verifier_rate_project_filter_isolates_matching_project() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_verifier_rate_filter").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_verifier_rate_project_filter_isolates_matching_project"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4().to_string();
    let session_a = "c0ffee00-0000-0000-0000-00000000fda1";
    let session_b = "c0ffee00-0000-0000-0000-00000000fda2";
    let project_a = format!("verifier-rate-filter-a-{run_marker}");
    let project_b = format!("verifier-rate-filter-b-{run_marker}");

    for (session_uuid, project_path) in [(session_a, &project_a), (session_b, &project_b)] {
        seed_project_scoped_intervention(
            &env,
            &client,
            session_uuid,
            project_path,
            &format!("verifier-rate filter test prompt for {project_path}"),
            &format!("verifier-rate-filter-anchor-{project_path}"),
        )
        .await?;
    }

    assert_project_filter_isolates_matching_row(&client, &env.inspect_url, &project_a, &project_b)
        .await?;

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

async fn mark_trigger_request_judge_failed(
    env: &ProxyWithDb,
    session_uuid: &str,
    trigger_request_id: i64,
) -> anyhow::Result<()> {
    let updated = sqlx::query(
        "UPDATE memory.operator_turns t
            SET judge_failure_reason = 'permanent'
           FROM sessions.sessions s
          WHERE t.session_id = s.id
            AND s.session_uuid = $1::uuid
            AND t.start_request_id <= $2
            AND t.end_request_id   >= $2",
    )
    .bind(session_uuid)
    .bind(trigger_request_id)
    .execute(&env.pool)
    .await?;
    anyhow::ensure!(
        updated.rows_affected() == 1,
        "expected exactly 1 auto-created operator_turns row covering request {trigger_request_id}, \
         updated {} — fixture assumption broken",
        updated.rows_affected()
    );
    Ok(())
}

#[tokio::test]
async fn get_verifier_rate_excludes_judge_failed_fires() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_verifier_rate_judge_failed").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping get_verifier_rate_excludes_judge_failed_fires"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4().to_string();
    let session_uuid = "c0ffee00-0000-0000-0000-00000000fda3";
    let project_path = format!("verifier-rate-judge-failed-{run_marker}");

    let trigger_request_id = seed_project_scoped_intervention(
        &env,
        &client,
        session_uuid,
        &project_path,
        &format!("verifier-rate judge-failed test prompt for {project_path}"),
        &format!("verifier-rate-judge-failed-anchor-{project_path}"),
    )
    .await?;

    mark_trigger_request_judge_failed(&env, session_uuid, trigger_request_id).await?;

    let resp = client
        .get(format!(
            "{}/api/steering/verifier-rate?project={project_path}",
            env.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for verifier-rate; got {}",
        resp.status()
    );
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows must be an array; got {body}"));
    let row = rows
        .iter()
        .find(|r| r["project_path"] == project_path.as_str())
        .unwrap_or_else(|| panic!("expected a row for {project_path}; got {body}"));

    assert_eq!(
        row["fires_count"], 0,
        "the judge-failed fire must be excluded from fires_count: {row}"
    );
    assert_eq!(
        row["excluded_judge_failed_count"], 1,
        "the judge-failed fire must be reported, not silently dropped: {row}"
    );
    assert_eq!(
        row["corrected_rate"],
        Value::Null,
        "corrected_rate must be null (0 non-excluded fires), not a deflated 0.0: {row}"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
