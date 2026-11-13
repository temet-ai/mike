use super::seed::{fixture, question, run, session};
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::questions::{AskedQuestion, SettleKind, SettledQuestion};

#[tokio::test]
async fn only_a_blocked_open_question_of_the_run_is_claimed_and_only_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    run(&f.handle, "run-a").await?;
    run(&f.handle, "run-b").await?;
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let blocked = AskedQuestion {
        run_id: Some("run-a"),
        blocked: true,
        ..question(asker, recipient, "blocked")
    };
    let wanted = f.handle.launch().ask_question(&blocked).await?;
    f.handle
        .launch()
        .ask_question(&AskedQuestion {
            blocked: false,
            question: "not blocked",
            ..blocked
        })
        .await?;
    f.handle
        .launch()
        .ask_question(&AskedQuestion {
            run_id: Some("run-b"),
            question: "another run",
            ..blocked
        })
        .await?;

    assert!(
        f.handle
            .launch()
            .escalate_due_questions("run-a", 5)
            .await?
            .is_empty(),
        "a question asked just now is not yet due"
    );
    let due = f.handle.launch().escalate_due_questions("run-a", 0).await?;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].question_uuid, wanted.question_uuid);
    assert!(due[0].escalated_at.is_some());
    assert!(
        f.handle
            .launch()
            .escalate_due_questions("run-a", 0)
            .await?
            .is_empty()
    );
    let other = f.handle.launch().escalate_due_questions("run-b", 0).await?;
    assert_eq!(other.len(), 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_settled_question_is_never_escalated_and_open_in_run_lists_only_open_ones()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    run(&f.handle, "run-c").await?;
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let base = AskedQuestion {
        run_id: Some("run-c"),
        blocked: true,
        ..question(asker, recipient, "answered soon")
    };
    let settled = f.handle.launch().ask_question(&base).await?;
    let open = f
        .handle
        .launch()
        .ask_question(&AskedQuestion {
            question: "still open",
            ..base
        })
        .await?;
    f.handle
        .launch()
        .settle_question(&SettledQuestion {
            question: settled.question_uuid,
            by: recipient,
            kind: SettleKind::Answered,
            answer: Some("done"),
        })
        .await?;

    let listed = f.handle.launch().open_questions_in_run("run-c").await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].question_uuid, open.question_uuid);
    let due = f.handle.launch().escalate_due_questions("run-c", 0).await?;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].question_uuid, open.question_uuid);
    assert!(
        f.handle
            .launch()
            .open_questions_in_run("no-such-run")
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}
