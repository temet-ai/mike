use crate::db_helpers;

use serde_json::{Value, json};

async fn post(
    client: &reqwest::Client,
    inspect_url: &str,
    project: &str,
    text: &str,
    topic: Option<&str>,
) -> anyhow::Result<Value> {
    let mut body = json!({"project_path": project, "kind": "warning", "text": text});
    if let Some(topic) = topic {
        body["topic"] = json!(topic);
    }
    let response = client
        .post(format!("{inspect_url}/api/lessons"))
        .json(&body)
        .send()
        .await?;
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    Ok(response.json().await?)
}

async fn proxy_with_proposer(
    proposer_on: bool,
) -> anyhow::Result<Option<(String, db_helpers::TestDbHandle, wiremock::MockServer)>> {
    let mock = wiremock::MockServer::start().await;
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    sqlx::query(
        "UPDATE config.feature_toggles SET enabled = $1, operator_set = true WHERE key = 'judge_lessons'",
    )
        .bind(proposer_on)
        .execute(&pool)
        .await?;
    let (_proxy_url, inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at(&mock.uri(), &test_db.database_url).await?;
    Ok(Some((inspect_url, test_db, mock)))
}

#[tokio::test]
async fn record_lesson_holds_a_lesson_that_is_not_a_rule_while_the_proposer_is_on()
-> anyhow::Result<()> {
    let Some((inspect_url, test_db, _mock)) = proxy_with_proposer(true).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let project = "test-lesson-shape";

    let rule = post(
        &client,
        &inspect_url,
        project,
        "Run scripts/gate lint before committing.",
        None,
    )
    .await?;
    assert!(
        rule.get("shape").is_none(),
        "a rule carries no shape finding: {rule}"
    );

    let status = post(
        &client,
        &inspect_url,
        project,
        "Satisfied. The ended session's style-check work was integrated and committed.",
        None,
    )
    .await?;
    assert_eq!(status["shape"]["finding"], "status", "{status}");
    assert_eq!(status["shape"]["held"], true);
    let detail = status["shape"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("\"Satisfied\""), "{detail}");

    let long = post(
        &client,
        &inspect_url,
        project,
        &"Verify every claim. ".repeat(40),
        None,
    )
    .await?;
    assert_eq!(long["shape"]["finding"], "over_length", "{long}");
    assert_eq!(long["shape"]["max_chars"], 500);
    assert_eq!(long["shape"]["chars"], 800);
    assert!(
        long["shape"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("not injected"),
        "{long}"
    );
    assert_eq!(
        long["text"].as_str().map(str::len),
        Some(800),
        "the text is stored and echoed whole"
    );

    let verifier = post(
        &client,
        &inspect_url,
        project,
        "Check the dispatch record.",
        Some("verifier:unmet"),
    )
    .await?;
    assert_eq!(verifier["shape"]["finding"], "status", "{verifier}");

    let listed: Vec<Value> = client
        .get(format!("{inspect_url}/api/lessons?project={project}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        listed.len(),
        4,
        "a flagged lesson is still listed: {listed:?}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_lesson_does_not_hold_anything_when_the_proposer_is_off() -> anyhow::Result<()> {
    let Some((inspect_url, test_db, _mock)) = proxy_with_proposer(false).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let long = post(
        &client,
        &inspect_url,
        "test-lesson-shape-off",
        &"Verify every claim. ".repeat(40),
        None,
    )
    .await?;

    assert_eq!(long["shape"]["finding"], "over_length", "{long}");
    assert_eq!(long["shape"]["held"], false);
    assert!(
        long["shape"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("lesson proposer is off"),
        "{long}"
    );
    test_db.cleanup().await?;
    Ok(())
}
