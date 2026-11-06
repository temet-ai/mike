use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn auto_enrol_mock_upstream() -> MockServer {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_auto_enrol_test")
                        .text("ack")
                        .usage(5, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}

fn auto_enrol_expected_project_path(label: &str) -> String {
    format!("/home/runner/projects/{label}")
}

async fn auto_enrol_register_project(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
    label: &str,
) -> anyhow::Result<()> {
    let cwd = auto_enrol_expected_project_path(label);
    let resp = client
        .post(format!("{inspect_url}/api/workspace/register"))
        .json(&serde_json::json!({
            "session_id": session_id,
            "cwd": cwd,
            "repo_root": cwd,
            "registered_via": "hook",
        }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "workspace register expected 2xx, got {}: {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
    Ok(())
}

async fn auto_enrol_send_with_project(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    label: &str,
) -> anyhow::Result<()> {
    let marker_path = format!("/home/runner/projects/{label}/CLAUDE.md");
    let system = format!(
        "Contents of {marker_path} (project instructions, checked into the codebase):\n\n# Rules"
    );
    let body = RequestBody::default()
        .system(system)
        .content("hello")
        .max_tokens(16)
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    Ok(())
}

async fn auto_enrol_wait_for_enrolled_steerer_ids(
    pool: &sqlx::PgPool,
    session_uuid: &str,
) -> anyhow::Result<Vec<i64>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT e.steerer_id
               FROM steering.enrolments e
               JOIN sessions.sessions   s ON s.id = e.session_id
              WHERE s.session_uuid = $1::uuid",
        )
        .bind(session_uuid)
        .fetch_all(pool)
        .await?;
        if !rows.is_empty() {
            return Ok(rows.into_iter().map(|(id,)| id).collect());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for auto-enrolment rows for session {session_uuid}; saw 0 rows"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn auto_enrol_respects_project_scope() -> anyhow::Result<()> {
    let upstream = auto_enrol_mock_upstream().await;

    let Some(env) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping auto_enrol_respects_project_scope");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let label_x = format!("auto-enrol-x-{run_id}");
    let label_y = format!("auto-enrol-y-{run_id}");
    let project_path_x = auto_enrol_expected_project_path(&label_x);

    let project_id_x: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(&project_path_x)
    .fetch_one(&env.pool)
    .await?;

    let global_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("auto-enrol-global-{run_id}"),
            "display_name": "Auto-enrol Test (global)",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
        }))
        .send()
        .await?;
    assert_eq!(
        global_resp.status(),
        reqwest::StatusCode::CREATED,
        "global steerer create should return 201"
    );
    let global_body: Value = global_resp.json().await?;
    let global_id = global_body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("global steerer.id must be a number; got {global_body}"))?;
    sqlx::query("UPDATE steering.steerers SET auto_enrol = true WHERE id = $1")
        .bind(global_id)
        .execute(&env.pool)
        .await?;

    let scoped_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("auto-enrol-scoped-{run_id}"),
            "display_name": "Auto-enrol Test (scoped X)",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
            "project_path": project_path_x,
        }))
        .send()
        .await?;
    assert_eq!(
        scoped_resp.status(),
        reqwest::StatusCode::CREATED,
        "scoped steerer create should return 201"
    );
    let scoped_body: Value = scoped_resp.json().await?;
    assert_eq!(
        scoped_body["steerer"]["project_id"].as_i64(),
        Some(project_id_x),
        "scoped steerer must resolve to project X's id; got {scoped_body}"
    );
    let scoped_id = scoped_body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("scoped steerer.id must be a number; got {scoped_body}"))?;
    sqlx::query("UPDATE steering.steerers SET auto_enrol = true WHERE id = $1")
        .bind(scoped_id)
        .execute(&env.pool)
        .await?;

    let session_x = uuid::Uuid::new_v4().to_string();
    let session_y = uuid::Uuid::new_v4().to_string();
    auto_enrol_register_project(&client, &env.inspect_url, &session_x, &label_x).await?;
    auto_enrol_register_project(&client, &env.inspect_url, &session_y, &label_y).await?;
    auto_enrol_send_with_project(&client, &env.proxy_url, &session_x, &label_x).await?;
    auto_enrol_send_with_project(&client, &env.proxy_url, &session_y, &label_y).await?;

    let session_x_steerers =
        auto_enrol_wait_for_enrolled_steerer_ids(&env.pool, &session_x).await?;
    let session_y_steerers =
        auto_enrol_wait_for_enrolled_steerer_ids(&env.pool, &session_y).await?;

    assert!(
        session_x_steerers.contains(&global_id),
        "session_x (project X) must be enrolled in the global auto_enrol steerer; got {session_x_steerers:?}"
    );
    assert!(
        session_x_steerers.contains(&scoped_id),
        "session_x (project X) must be enrolled in the project-X-scoped auto_enrol steerer; got {session_x_steerers:?}"
    );
    assert!(
        session_y_steerers.contains(&global_id),
        "session_y (project Y) must be enrolled in the global auto_enrol steerer regardless of project; got {session_y_steerers:?}"
    );
    assert!(
        !session_y_steerers.contains(&scoped_id),
        "session_y (project Y) must NOT be enrolled in the project-X-scoped auto_enrol steerer; got {session_y_steerers:?}"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
