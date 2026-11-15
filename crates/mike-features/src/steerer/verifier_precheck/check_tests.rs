use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::facts::{Facts, Sources};
use super::holdback::sent_anyway;
use super::rules::{MAILBOX_NUDGE, RULES_MODEL};
use super::{FireInput, POINT, Subject, check, mark};
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;
use crate::steerer::verdict::{Need, SteererVerdict};
use crate::store::persistence::PersistHandle;
use crate::store::test_support::TestDb;

const MAILBOX: &str = "You have an unread cross-session mailbox message (status: pending, unclaimed). Check your mailbox before treating this session as done.";

async fn never_called() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    server
}

fn decider(server: &MockServer, persist: &PersistHandle) -> anyhow::Result<Decider> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Decider::with_mode(
        &client,
        persist,
        &SettingsOverlay::default(),
        POINT,
        DecisionMode::On,
    )
    .ok_or_else(|| anyhow::anyhow!("no decider"))
}

fn evaluation_where(holdback: bool) -> Uuid {
    loop {
        let uuid = Uuid::new_v4();
        if sent_anyway(Some(uuid)) == holdback {
            return uuid;
        }
    }
}

fn quiet_verdict() -> SteererVerdict {
    SteererVerdict {
        fires: false,
        confidence: 0.2,
        reason: String::new(),
        nudge: String::new(),
        need: Need::Low,
        checklist: Vec::new(),
    }
}

fn mailbox_facts(verdict: &SteererVerdict) -> Facts {
    let sources = Sources {
        nudge: MAILBOX,
        verdict,
        criteria: &[],
        accumulated_ask: "[initial] fix the clearance",
        operator_ask: "fix the clearance",
        last_user_message: "",
    };
    Facts::derive(&sources, Vec::new())
}

fn subject(evaluation: Uuid) -> Subject {
    Subject {
        session_db_id: None,
        operator_turn_id: None,
        request_id: None,
        evaluation: Some(evaluation),
    }
}

async fn stored(
    persist: &PersistHandle,
    evaluation: Uuid,
) -> anyhow::Result<(String, String, Value)> {
    Ok(sqlx::query_as::<_, (String, String, Value)>(
        "SELECT model_requested, applied, composed FROM decisions.records
          WHERE evaluation_uuid = $1 AND point = $2",
    )
    .bind(evaluation)
    .bind(POINT)
    .fetch_one(persist.pool())
    .await?)
}

#[tokio::test]
async fn check_mailbox_nudge_is_held_by_code_without_asking_jev() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = never_called().await;
    let decider = decider(&server, &persist)?;
    let verdict = quiet_verdict();
    let input = FireInput {
        operator_ask: "fix the clearance",
        turn_end: "All done.",
        nudge: MAILBOX,
        verdict: &verdict,
    };
    let evaluation = evaluation_where(false);
    let facts = mailbox_facts(&verdict);
    let Some(checked) = check(&decider, &input, &facts, &subject(evaluation)).await else {
        anyhow::bail!("a code rule must settle the fire");
    };
    assert!(checked.verdict.holds());
    assert_eq!(checked.verdict.rule, Some(MAILBOX_NUDGE));
    mark(&decider, &checked, false).await;
    let (model, applied, composed) = stored(&persist, evaluation).await?;
    assert_eq!(model, RULES_MODEL);
    assert_eq!(applied, "held");
    assert_eq!(composed["verdict"]["rule"], json!(MAILBOX_NUDGE));
    assert_eq!(composed["verdict"]["send"], json!(false));
    assert_eq!(composed["ask_source"], json!("operator"));
    Ok(())
}

#[tokio::test]
async fn check_held_fire_drawn_for_holdback_is_sent_and_marked_a_hold() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire().await?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let server = never_called().await;
    let decider = decider(&server, &persist)?;
    let verdict = quiet_verdict();
    let input = FireInput {
        operator_ask: "fix the clearance",
        turn_end: "All done.",
        nudge: MAILBOX,
        verdict: &verdict,
    };
    let evaluation = evaluation_where(true);
    let facts = mailbox_facts(&verdict);
    let Some(checked) = check(&decider, &input, &facts, &subject(evaluation)).await else {
        anyhow::bail!("a code rule must settle the fire");
    };
    assert!(!checked.verdict.holds());
    assert!(checked.verdict.holdback);
    mark(&decider, &checked, true).await;
    let (_, applied, composed) = stored(&persist, evaluation).await?;
    assert_eq!(applied, "sent");
    assert_eq!(composed["verdict"]["send"], json!(false));
    assert_eq!(composed["verdict"]["holdback"], json!(true));
    Ok(())
}
