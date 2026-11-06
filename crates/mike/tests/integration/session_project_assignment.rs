use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn register(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    cwd: &str,
    repo_root: Option<&str>,
) -> anyhow::Result<Value> {
    let mut body = json!({"session_id": session_id, "cwd": cwd});
    if let Some(root) = repo_root {
        body["repo_root"] = json!(root);
    }
    let response = client
        .post(format!("{inspect_url}/api/workspace/register"))
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "registration failed: {}",
        response.status()
    );
    Ok(response.json().await?)
}

async fn send_marker_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: uuid::Uuid,
    marker: &str,
) -> anyhow::Result<()> {
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id.to_string())
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(
            &RequestBody::default()
                .system(format!(
                    "Working directory: {marker}\nIs a git repository: true"
                ))
                .content("hello")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "proxy request failed: {}",
        response.status()
    );
    Ok(())
}

async fn assert_unresolved_registration(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    cwd: &str,
) -> anyhow::Result<()> {
    let unresolved = register(client, inspect_url, session_id, cwd, None).await?;
    assert!(unresolved["project_id"].is_null(), "{unresolved}");
    assert!(unresolved["project_path"].is_null(), "{unresolved}");
    assert_eq!(unresolved["project_resolution"], "unresolved");
    assert_eq!(unresolved["cwd"], cwd);
    assert_eq!(unresolved["roster"].as_array().map(Vec::len), Some(1));
    assert!(
        unresolved["briefing"]
            .as_str()
            .unwrap_or_default()
            .contains("Ask the user")
    );
    Ok(())
}

async fn assert_marker_request_does_not_mint_project(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    session_id: uuid::Uuid,
    forged_root: &str,
) -> anyhow::Result<()> {
    send_marker_request(client, proxy_url, session_id, forged_root).await?;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let forged_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM projects.projects WHERE project_path = $1",
    )
    .bind(forged_root)
    .fetch_one(pool)
    .await?;
    assert_eq!(forged_count, 0, "request marker must not mint a project");
    Ok(())
}

async fn assert_operator_assignment_survives_reregistration(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    git_root: &str,
) -> anyhow::Result<String> {
    let resolved = register(
        client,
        inspect_url,
        session_id,
        "/tmp/repos-a/app/subdir",
        Some(git_root),
    )
    .await?;
    assert_eq!(resolved["project_path"], git_root);
    assert_eq!(resolved["project_resolution"], "git");

    let operator_root = format!("/tmp/repos-b-{}/app", uuid::Uuid::new_v4());
    let assignment = client
        .patch(format!("{inspect_url}/api/sessions/{session_id}/project"))
        .json(&json!({"project_path": operator_root}))
        .send()
        .await?;
    assert_eq!(assignment.status(), reqwest::StatusCode::OK);
    let assignment: Value = assignment.json().await?;
    assert_eq!(assignment["project_resolution"], "operator");
    assert_eq!(assignment["project_path"], operator_root);

    let reregistered = register(
        client,
        inspect_url,
        session_id,
        "/tmp/repos-a/app/again",
        Some(git_root),
    )
    .await?;
    assert_eq!(reregistered["project_path"], operator_root);
    assert_eq!(reregistered["project_resolution"], "operator");

    Ok(operator_root)
}

#[tokio::test]
async fn session_project_identity_is_registration_and_operator_owned() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                MockResponse::new("msg_project_identity")
                    .text("ack")
                    .usage(5, 1)
                    .build(),
            ),
        )
        .mount(&upstream)
        .await;

    let Some(handle) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_project_assignment");
        return Ok(());
    };
    let client = reqwest::Client::new();
    let session_id = uuid::Uuid::new_v4();
    let unresolved_cwd = format!("/tmp/not-a-repo-{}", uuid::Uuid::new_v4());
    assert_unresolved_registration(&client, &handle.inspect_url, session_id, &unresolved_cwd)
        .await?;

    let forged_root = format!("/tmp/forged-{}", uuid::Uuid::new_v4());
    assert_marker_request_does_not_mint_project(
        &client,
        &handle.proxy_url,
        &handle.pool,
        session_id,
        &forged_root,
    )
    .await?;

    let git_root = format!("/tmp/repos-a-{}/app", uuid::Uuid::new_v4());
    let operator_root = assert_operator_assignment_survives_reregistration(
        &client,
        &handle.inspect_url,
        session_id,
        &git_root,
    )
    .await?;

    let same_basename_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM projects.projects WHERE project_path = ANY($1)",
    )
    .bind(vec![git_root.clone(), operator_root.clone()])
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        same_basename_count, 2,
        "same-basename roots remain distinct"
    );

    let request_project: String = sqlx::query_scalar(
        "SELECT p.project_path
           FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
           JOIN projects.projects p ON p.id = r.project_id
          WHERE s.session_uuid = $1
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        request_project, operator_root,
        "manual assignment backfills requests"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
