use super::*;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

async fn project_supplied_for(summary: Value) -> Result<Option<String>, ErrorData> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "summary": summary })))
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let supplied = supply_defaults(&client, "get_workspace_roster", None, Some(SESSION)).await?;
    Ok(supplied.and_then(|arguments| {
        arguments
            .get("project_path")
            .and_then(Value::as_str)
            .map(str::to_string)
    }))
}

#[tokio::test]
async fn caller_project_without_a_project_falls_back_to_the_session_cwd() -> anyhow::Result<()> {
    for project in [Value::Null, json!(""), json!("(no project)")] {
        let supplied = project_supplied_for(
            json!({"id": SESSION, "project": project, "cwd": "/work/checkout"}),
        )
        .await?;
        assert_eq!(supplied.as_deref(), Some("/work/checkout"), "{project}");
    }
    Ok(())
}

#[tokio::test]
async fn caller_project_prefers_the_recorded_project_over_the_cwd() -> anyhow::Result<()> {
    let supplied = project_supplied_for(
        json!({"id": SESSION, "project": "/recorded/project", "cwd": "/work/checkout"}),
    )
    .await?;
    assert_eq!(supplied.as_deref(), Some("/recorded/project"));
    Ok(())
}

#[tokio::test]
async fn caller_project_with_a_relative_or_missing_cwd_is_an_error() {
    for cwd in [Value::Null, json!("relative/dir")] {
        let supplied =
            project_supplied_for(json!({"id": SESSION, "project": null, "cwd": cwd})).await;
        assert!(supplied.is_err(), "{cwd}");
    }
}
