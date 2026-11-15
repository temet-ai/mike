use clap::Parser;

use super::*;
use crate::config::ProxyConfig;
use crate::config::load::CliOverrides;

fn parse(args: &[&str]) -> anyhow::Result<ProxyConfig> {
    let overrides = CliOverrides::try_parse_from(args)?;
    crate::config::load::load_with(overrides, &crate::environment::MapEnvironment::isolated())
}

#[test]
fn answer_without_applying_reports_the_layer_off() {
    let off = answer(Some("folder_trust"), false);
    assert_eq!(off.layer, "off");
    let on = answer(Some("folder_trust"), true);
    assert_eq!(on.layer, "on");
    assert_eq!(on.dialog.as_deref(), Some("folder_trust"));
}

#[test]
fn context_of_a_request_with_a_session_carries_it() {
    let request = BlockedDialogRequest {
        screen: "x".to_owned(),
        session: Some("abc".to_owned()),
    };
    assert_eq!(context_of(&request), json!({"session": "abc"}));
}

#[tokio::test]
async fn post_blocked_dialog_with_the_layer_off_answers_off_and_no_dialog() -> anyhow::Result<()> {
    let config = parse(&["mike", "--persist-enabled", "0"])?;
    let state = AppState::new(config).await?;
    let request = BlockedDialogRequest {
        screen: "Do you trust the files in this folder?".to_owned(),
        session: None,
    };
    let Json(reply) = post_blocked_dialog(State(state), Json(request)).await;
    assert_eq!(reply.layer, "off");
    assert_eq!(reply.dialog, None);
    Ok(())
}
