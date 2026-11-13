#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use super::FakeHerdr;
use crate::launch::herdr::channels_dialog::{ChannelsDialogOutcome, confirm};
use crate::launch::herdr::cli::HerdrCli;

#[tokio::test]
async fn confirms_by_sending_enter_once_the_dialog_text_appears() {
    let fake = FakeHerdr::new();
    fake.answer("pane wait-output", "{\"id\":\"cli:fake\",\"result\":{}}");
    fake.answer("pane send-keys", "{\"id\":\"cli:fake\",\"result\":{}}");
    let cli = HerdrCli::new(fake.bin());

    let outcome = confirm(&cli, "w1:p1").await;

    assert_eq!(outcome, ChannelsDialogOutcome::Confirmed);
    let calls = fake.calls();
    assert_eq!(
        calls[0][..2],
        ["pane".to_string(), "wait-output".to_string()]
    );
    assert!(calls[0].iter().any(|a| a == "Loading development channels"));
    assert_eq!(calls[1][..2], ["pane".to_string(), "send-keys".to_string()]);
    assert!(calls[1].iter().any(|a| a == "enter"));
}

#[tokio::test]
async fn never_sends_a_blind_enter_when_the_dialog_text_never_appears() {
    let fake = FakeHerdr::new();
    fake.error(
        "pane wait-output",
        "timeout",
        "no match within the deadline",
    );
    let cli = HerdrCli::new(fake.bin());

    let outcome = confirm(&cli, "w1:p1").await;

    assert_eq!(outcome, ChannelsDialogOutcome::NotSeen);
    let calls = fake.calls();
    assert_eq!(
        calls.len(),
        1,
        "send-keys must never run when the dialog was not seen"
    );
}

#[tokio::test]
async fn reports_not_seen_when_send_keys_itself_fails() {
    let fake = FakeHerdr::new();
    fake.answer("pane wait-output", "{\"id\":\"cli:fake\",\"result\":{}}");
    fake.error("pane send-keys", "pane_not_found", "pane closed mid-flight");
    let cli = HerdrCli::new(fake.bin());

    let outcome = confirm(&cli, "w1:p1").await;

    assert_eq!(outcome, ChannelsDialogOutcome::NotSeen);
}
