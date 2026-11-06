use crate::db_helpers;
use crate::db_helpers::ProxyWithDb;
use serde_json::Value;

use super::support::{
    fire_ensures_session_exists, fire_request_and_get_id, mount_minimal_upstream,
};

#[tokio::test]
async fn get_session_enrolments_returns_auto_enrolled_steerers_for_new_session()
-> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_session_enrolments").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_session_enrolments_returns_auto_enrolled_steerers_for_new_session"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-000000000003";

    fire_ensures_session_exists(
        &client,
        &env.proxy_url,
        session_uuid,
        "steering api test — session enrolments empty",
    )
    .await?;

    let mut body: Value = serde_json::json!([]);
    for _ in 0..50 {
        let enrolments_resp = client
            .get(format!(
                "{}/api/sessions/{}/enrolments",
                env.inspect_url, session_uuid
            ))
            .send()
            .await?;
        assert_eq!(
            enrolments_resp.status(),
            reqwest::StatusCode::OK,
            "expected 200 for session enrolments; got {}",
            enrolments_resp.status()
        );
        body = enrolments_resp.json().await?;
        if body.as_array().is_some_and(|a| !a.is_empty()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let Some(enrolments) = body.as_array() else {
        anyhow::bail!("enrolments endpoint must return a JSON array; got {body}");
    };
    let Some(verifier) = enrolments.iter().find(|e| e["steerer_name"] == "verifier") else {
        anyhow::bail!("expected auto-enrolled verifier steerer; got {body}");
    };
    anyhow::ensure!(
        verifier["enrolled_by"] == "auto",
        "verifier enrolment must be auto-enrolled; got {body}"
    );
    anyhow::ensure!(
        verifier["disenrolled_at"].is_null(),
        "auto-enrolled verifier must be active; got {body}"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

async fn setup_cross_project_steerer_fixture(
    env: &ProxyWithDb,
    client: &reqwest::Client,
    session_uuid: &str,
    project_a: &str,
    project_b: &str,
    run_marker: &str,
) -> anyhow::Result<i64> {
    let _trigger_request_id = fire_request_and_get_id(
        client,
        &env.proxy_url,
        &env.pool,
        session_uuid,
        &format!("cross-project enrol test prompt {run_marker}"),
    )
    .await?;
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
    .bind(project_a)
    .execute(&env.pool)
    .await?;

    let project_b_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(project_b)
    .fetch_one(&env.pool)
    .await?;

    let create_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("cross-project-enrol-steerer-{run_marker}"),
            "display_name": "Cross-Project Enrol Steerer",
            "authority": "redirect",
            "mode": "augment",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
            "project_path": project_b,
        }))
        .send()
        .await?;
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let create_body: Value = create_resp.json().await?;
    assert_eq!(
        create_body["steerer"]["project_id"].as_i64(),
        Some(project_b_id),
        "steerer must be scoped to project B; got {create_body}"
    );
    create_body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("steerer.id must be a number"))
}

#[tokio::test]
async fn post_session_enrolments_rejects_cross_project_manual_enrol() -> anyhow::Result<()> {
    let upstream = mount_minimal_upstream("msg_test_cross_project_enrol").await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             post_session_enrolments_rejects_cross_project_manual_enrol"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_marker = uuid::Uuid::new_v4().to_string();
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let project_a = format!("cross-project-enrol-a-{run_marker}");
    let project_b = format!("cross-project-enrol-b-{run_marker}");

    let steerer_id = setup_cross_project_steerer_fixture(
        &env,
        &client,
        &session_uuid,
        &project_a,
        &project_b,
        &run_marker,
    )
    .await?;

    let enrol_resp = client
        .post(format!(
            "{}/api/sessions/{session_uuid}/enrolments",
            env.inspect_url
        ))
        .json(&serde_json::json!({
            "steerer_id": steerer_id,
            "mode": "augment",
            "authority": "redirect",
        }))
        .send()
        .await?;
    assert!(
        enrol_resp.status().is_client_error(),
        "cross-project manual enrol must be rejected with a 4xx; got {}",
        enrol_resp.status()
    );

    let enrolment_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM steering.enrolments e
          JOIN sessions.sessions s ON s.id = e.session_id
         WHERE s.session_uuid = $1::uuid
           AND e.steerer_id   = $2",
    )
    .bind(&session_uuid)
    .bind(steerer_id)
    .fetch_one(&env.pool)
    .await?;
    assert_eq!(
        enrolment_count, 0,
        "a rejected cross-project enrol must not insert an enrolment row"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
