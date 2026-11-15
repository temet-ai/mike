use std::collections::BTreeMap;

use super::*;
use crate::launch::cli::test_kit::fake_proxy;
use crate::launch::herdr::prompt::blocked_jev::{compose, dialog_via};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

const TRUST: &str = "Do you trust the files in this folder?\n1. Yes, proceed\n2. No, exit\n";

fn route(status: u16, body: &str) -> (&'static str, String, u16, String) {
    ("POST", ROUTE.to_owned(), status, body.to_owned())
}

fn on(dialog: &str) -> BlockedDialogAnswer {
    BlockedDialogAnswer {
        dialog: Some(dialog.to_owned()),
        layer: "on".to_owned(),
    }
}

fn local_answer(key: &str) -> Asked {
    Asked::answered(BTreeMap::from([(
        "which".to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), 0.9)]),
            confidence: None,
        },
    )]))
}

#[test]
fn dialog_named_by_an_on_answer_matches_a_local_answer() {
    let local = compose(&local_answer("folder_trust")).map(|(name, _)| name.to_owned());
    assert_eq!(dialog_named_by(&on("folder_trust")), local);
    assert_eq!(local.as_deref(), Some("the folder trust dialog"));
}

#[test]
fn dialog_named_by_an_off_layer_gives_no_name() {
    let answer = BlockedDialogAnswer {
        dialog: Some("folder_trust".to_owned()),
        layer: "off".to_owned(),
    };
    assert_eq!(dialog_named_by(&answer), None);
}

#[test]
fn dialog_named_by_an_unknown_key_gives_no_name() {
    assert_eq!(dialog_named_by(&on("something_else")), None);
}

#[tokio::test]
async fn ask_an_answering_proxy_names_the_dialog() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![route(
        200,
        r#"{"dialog":"permission_prompt","layer":"on"}"#,
    )])
    .await?;
    let found = ask(&format!("http://{}", proxy.addr), TRUST).await;
    assert_eq!(found.as_deref(), Some("a tool permission prompt"));
    Ok(())
}

#[tokio::test]
async fn ask_a_proxy_error_gives_no_name() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![route(500, "{}")]).await?;
    assert_eq!(ask(&format!("http://{}", proxy.addr), TRUST).await, None);
    Ok(())
}

#[tokio::test]
async fn ask_an_unreachable_proxy_gives_no_name() {
    assert_eq!(ask("http://127.0.0.1:1", TRUST).await, None);
}

#[tokio::test]
async fn dialog_via_a_failing_proxy_uses_the_screen_rule() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![route(500, "{}")]).await?;
    let base = format!("http://{}", proxy.addr);
    let found = dialog_via(None, Some(&base), TRUST).await;
    assert_eq!(found.as_deref(), Some("the folder trust dialog"));
    Ok(())
}

#[tokio::test]
async fn dialog_via_an_off_layer_uses_the_screen_rule() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![route(200, r#"{"dialog":null,"layer":"off"}"#)]).await?;
    let base = format!("http://{}", proxy.addr);
    let found = dialog_via(None, Some(&base), TRUST).await;
    assert_eq!(found.as_deref(), Some("the folder trust dialog"));
    Ok(())
}

#[tokio::test]
async fn dialog_via_a_proxy_answer_takes_the_proxys_dialog() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![route(
        200,
        r#"{"dialog":"permission_prompt","layer":"on"}"#,
    )])
    .await?;
    let base = format!("http://{}", proxy.addr);
    let found = dialog_via(None, Some(&base), TRUST).await;
    assert_eq!(found.as_deref(), Some("a tool permission prompt"));
    Ok(())
}
