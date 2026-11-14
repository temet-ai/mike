use std::sync::Arc;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::decisions::env::DecisionEnv;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn answer(skill: &str) -> Value {
    json!({"type": "choice", "choice": skill, "probabilities": {skill: 0.9}})
}

async fn stub(first: &str, second: &str) -> MockServer {
    let server = MockServer::start().await;
    let body = json!({
        "model": "typesafe/jev-1.14",
        "answers": {"message_1": answer(first), "message_2": answer(second)},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    });
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn queries() -> Vec<TriggerQuery> {
    vec![
        TriggerQuery {
            query: "deploy the proxy".to_owned(),
            should_trigger: true,
        },
        TriggerQuery {
            query: "rename a variable".to_owned(),
            should_trigger: false,
        },
    ]
}

async fn labels(persist: &PersistHandle) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query!(
        "SELECT o.label, o.label_source FROM decisions.outcomes o
           JOIN decisions.records r ON r.id = o.record_id
          WHERE r.point = 'skill_trigger'"
    )
    .fetch_all(persist.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.label, r.label_source))
        .collect())
}

async fn run(first: &str, second: &str) -> anyhow::Result<Option<Vec<(String, String)>>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = stub(first, second).await;
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    let env = DecisionEnv::new(
        client.clone(),
        persist.clone(),
        true,
        Arc::new(SettingsOverlay::default()),
    );
    let jev = JevTrigger::new(Some(&env)).ok_or_else(|| anyhow::anyhow!("no decider"))?;
    let entries = vec![
        ("deploy-proxy".to_owned(), "Deploy the proxy".to_owned()),
        ("db-migrations".to_owned(), "Write migrations".to_owned()),
    ];
    let offer = Offer {
        name: "deploy-proxy",
        entries: &entries,
        project: "a project",
    };
    let queries = queries();
    let picked: Vec<&TriggerQuery> = queries.iter().collect();
    ask_batch(&client, Some(&jev), &offer, 0, &picked).await?;
    let found = labels(&persist).await?;
    persist.pool().close().await;
    db.cleanup().await?;
    Ok(Some(found))
}

#[tokio::test]
async fn skill_trigger_batch_matching_the_expected_triggers_is_labelled_right() -> anyhow::Result<()>
{
    let Some(found) = run("deploy-proxy", "none").await? else {
        return Ok(());
    };
    assert_eq!(
        found,
        vec![("right".to_owned(), "expected_trigger".to_owned())]
    );
    Ok(())
}

#[tokio::test]
async fn skill_trigger_batch_contradicting_the_expected_triggers_is_labelled_wrong()
-> anyhow::Result<()> {
    let Some(found) = run("none", "deploy-proxy").await? else {
        return Ok(());
    };
    assert_eq!(
        found,
        vec![("wrong".to_owned(), "expected_trigger".to_owned())]
    );
    Ok(())
}
