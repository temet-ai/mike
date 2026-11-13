use std::time::Duration;

use serde_json::json;

use super::{FakeHerdr, agent_entry, envelope};
use crate::launch::herdr::cli::HerdrCli;
use crate::launch::herdr::prompt::{Pacing, PromptOutcome, deliver_paced};

const NAME: &str = "cc-11111111";
const PROMPT: &str = "Reply with the single word ok.";
const UNSENT: &str = "welcome\n────────\n❯ Reply with the single word ok.\n────────\n";
const SENT: &str = "welcome\n────────\n❯\n────────\n";
const PASTED: &str = "welcome\n────────\n❯ [Pasted text #1 +9 lines]\n────────\n";
const TRUST_DIALOG: &str = "Do you trust the files in this folder?\n1. Yes, proceed\n2. No, exit\n";

const INSTANT: Pacing = Pacing {
    poll: Duration::ZERO,
    block_polls: 2,
    settle_polls: 1,
    rounds: 2,
};

fn agent_reply(status: &str) -> String {
    envelope(json!({"agent": agent_entry("w1:p2", "claude", None, status), "type": "agent_info"}))
}

fn prompted(status: &str) -> String {
    envelope(
        json!({"agent": agent_entry("w1:p2", "claude", None, status), "type": "agent_prompted"}),
    )
}

fn call_count(fake: &FakeHerdr, head: [&str; 2]) -> usize {
    fake.calls()
        .iter()
        .filter(|c| c.len() >= 2 && c[0] == head[0] && c[1] == head[1])
        .count()
}

async fn run(fake: &FakeHerdr) -> anyhow::Result<PromptOutcome> {
    let cli = HerdrCli::new(fake.bin());
    Ok(deliver_paced(&cli, NAME, PROMPT, 30_000, INSTANT).await?)
}

fn stalled(fake: &FakeHerdr) {
    fake.error(
        "agent prompt",
        "agent_prompt_stalled",
        "no working or blocked state within 5000ms",
    );
}

#[tokio::test]
async fn prompt_stalled_then_second_enter_submits_it() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    stalled(&fake);
    fake.answer("agent read", UNSENT)
        .answer("agent send-keys", &envelope(json!({})))
        .answer("pane send-text", &envelope(json!({})))
        .answer("agent get", &agent_reply("idle"))
        .answer("agent get", &agent_reply("idle"))
        .answer("agent get", &agent_reply("working"));
    let outcome = run(&fake).await?;
    assert!(outcome.delivered && outcome.confirmed, "{outcome:?}");
    assert_eq!(outcome.agent_state.as_deref(), Some("working"));
    assert_eq!(call_count(&fake, ["agent", "send-keys"]), 2);
    assert!(
        fake.calls()
            .iter()
            .any(|c| c == &["pane", "send-text", "w1:p2", "\r"])
    );
    Ok(())
}

#[tokio::test]
async fn prompt_stalled_then_first_enter_submits_it_without_carriage_return() -> anyhow::Result<()>
{
    let fake = FakeHerdr::new();
    stalled(&fake);
    fake.answer("agent read", UNSENT)
        .answer("agent send-keys", &envelope(json!({})))
        .answer("agent get", &agent_reply("working"));
    let outcome = run(&fake).await?;
    assert!(outcome.confirmed, "{outcome:?}");
    assert_eq!(call_count(&fake, ["agent", "send-keys"]), 1);
    assert_eq!(call_count(&fake, ["pane", "send-text"]), 0);
    Ok(())
}

#[tokio::test]
async fn prompt_stalled_with_text_no_longer_in_input_sends_no_keys() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    stalled(&fake);
    fake.answer("agent read", SENT);
    let outcome = run(&fake).await?;
    assert!(outcome.delivered && !outcome.confirmed, "{outcome:?}");
    assert_eq!(call_count(&fake, ["agent", "send-keys"]), 0);
    Ok(())
}

#[tokio::test]
async fn prompt_stalled_pasted_placeholder_counts_as_unsubmitted() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    stalled(&fake);
    fake.answer("agent read", PASTED)
        .answer("agent send-keys", &envelope(json!({})))
        .answer("agent get", &agent_reply("working"));
    let outcome = run(&fake).await?;
    assert!(outcome.confirmed, "{outcome:?}");
    Ok(())
}

#[tokio::test]
async fn prompt_stalled_and_never_submitted_says_how_many_presses_failed() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    stalled(&fake);
    fake.answer("agent read", UNSENT)
        .answer("agent send-keys", &envelope(json!({})))
        .answer("pane send-text", &envelope(json!({})))
        .answer("agent get", &agent_reply("idle"));
    let outcome = run(&fake).await?;
    assert!(outcome.delivered && !outcome.confirmed, "{outcome:?}");
    let detail = outcome.detail.unwrap_or_default();
    assert!(detail.contains("agent_prompt_stalled"), "{detail}");
    assert!(
        detail.contains("unsubmitted in the input after 2"),
        "{detail}"
    );
    Ok(())
}

#[tokio::test]
async fn prompt_stalled_with_unreadable_pane_stays_unconfirmed() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    stalled(&fake);
    let outcome = run(&fake).await?;
    assert!(outcome.delivered && !outcome.confirmed, "{outcome:?}");
    assert_eq!(call_count(&fake, ["agent", "send-keys"]), 0);
    Ok(())
}

#[tokio::test]
async fn prompt_blocked_then_cleared_is_delivered_on_the_second_try() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    fake.error(
        "agent prompt",
        "agent_blocked",
        "agent is blocked on a dialog",
    )
    .answer("agent prompt", &prompted("working"))
    .answer("agent get", &agent_reply("blocked"))
    .answer("agent get", &agent_reply("idle"));
    let outcome = run(&fake).await?;
    assert!(outcome.delivered && outcome.confirmed, "{outcome:?}");
    assert_eq!(call_count(&fake, ["agent", "prompt"]), 2);
    assert_eq!(call_count(&fake, ["agent", "get"]), 2);
    Ok(())
}

#[tokio::test]
async fn prompt_blocked_by_a_dialog_that_stays_names_the_dialog() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    fake.error(
        "agent prompt",
        "agent_blocked",
        "agent is blocked on a dialog",
    )
    .answer("agent get", &agent_reply("blocked"))
    .answer("agent read", TRUST_DIALOG);
    let outcome = run(&fake).await?;
    assert!(!outcome.delivered && !outcome.confirmed, "{outcome:?}");
    assert_eq!(outcome.agent_state.as_deref(), Some("blocked"));
    let detail = outcome.detail.unwrap_or_default();
    assert!(detail.contains("blocked on a dialog"), "{detail}");
    assert!(detail.contains("the folder trust dialog"), "{detail}");
    assert!(detail.contains("needs a person"), "{detail}");
    assert_eq!(call_count(&fake, ["agent", "prompt"]), 1);
    assert_eq!(call_count(&fake, ["agent", "get"]), 2);
    Ok(())
}

#[tokio::test]
async fn prompt_blocked_by_an_unknown_dialog_quotes_its_question() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    fake.error(
        "agent prompt",
        "agent_blocked",
        "agent is blocked on a dialog",
    )
    .answer("agent get", &agent_reply("blocked"))
    .answer("agent read", "Enable auto mode?\n1. Yes\n2. No\n");
    let outcome = run(&fake).await?;
    let detail = outcome.detail.unwrap_or_default();
    assert!(detail.contains("\"Enable auto mode?\""), "{detail}");
    Ok(())
}

#[tokio::test]
async fn prompt_blocked_with_unreadable_pane_says_so() -> anyhow::Result<()> {
    let fake = FakeHerdr::new();
    fake.error(
        "agent prompt",
        "agent_blocked",
        "agent is blocked on a dialog",
    );
    let outcome = run(&fake).await?;
    assert!(!outcome.delivered, "{outcome:?}");
    let detail = outcome.detail.unwrap_or_default();
    assert!(detail.contains("the pane could not be read"), "{detail}");
    Ok(())
}
