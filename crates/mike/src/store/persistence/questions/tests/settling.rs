use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::seed::{fixture, question, session};
use crate::store::persistence::PersistError;
use crate::store::persistence::questions::{
    STATE_ANSWERED, STATE_DECLINED, SettleKind, SettleOutcome, SettledQuestion,
};

#[tokio::test]
async fn an_answer_settles_once_and_leaves_the_recipients_open_list() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let asked = f
        .handle
        .launch()
        .ask_question(&question(asker, recipient, "which file?"))
        .await?;

    let first = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: asked.question_uuid,
            by: recipient,
            kind: SettleKind::Answered,
            answer: Some("  fleet_target.rs  "),
        })
        .await?;
    let SettleOutcome::Settled(row) = first else {
        anyhow::bail!("expected Settled, got {first:?}");
    };
    assert_eq!(row.state, STATE_ANSWERED);
    assert_eq!(row.answer.as_deref(), Some("fleet_target.rs"));
    assert_eq!(row.settled_by, Some(recipient));
    assert!(row.settled_at.is_some());
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );

    let again = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: asked.question_uuid,
            by: recipient,
            kind: SettleKind::Declined,
            answer: None,
        })
        .await?;
    let SettleOutcome::AlreadySettled(kept) = again else {
        anyhow::bail!("expected AlreadySettled, got {again:?}");
    };
    assert_eq!(kept.state, STATE_ANSWERED);
    assert_eq!(kept.answer.as_deref(), Some("fleet_target.rs"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_decline_needs_no_reason_and_anyone_in_the_project_may_settle() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let reference = session(&f.handle).await?;
    let asked = f
        .handle
        .launch()
        .ask_question(&question(asker, recipient, "can you?"))
        .await?;
    let outcome = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: asked.question_uuid,
            by: reference,
            kind: SettleKind::Declined,
            answer: None,
        })
        .await?;
    let SettleOutcome::Settled(row) = outcome else {
        anyhow::bail!("expected Settled, got {outcome:?}");
    };
    assert_eq!(row.state, STATE_DECLINED);
    assert_eq!(row.settled_by, Some(reference));
    assert!(row.answer.is_none());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn settling_refuses_an_empty_answer_an_unknown_settler_and_reports_an_unknown_question()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    let asked = f
        .handle
        .launch()
        .ask_question(&question(asker, recipient, "q"))
        .await?;

    let empty = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: asked.question_uuid,
            by: recipient,
            kind: SettleKind::Answered,
            answer: Some("   "),
        })
        .await;
    assert!(matches!(&empty, Err(PersistError::DataInvariant(m)) if m.contains("answer")));

    let stranger = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: asked.question_uuid,
            by: Uuid::new_v4(),
            kind: SettleKind::Answered,
            answer: Some("x"),
        })
        .await;
    assert!(matches!(&stranger, Err(PersistError::DataInvariant(m)) if m.contains("settler")));

    let missing = f
        .handle
        .launch()
        .settle_question(&SettledQuestion {
            question: Uuid::new_v4(),
            by: recipient,
            kind: SettleKind::Answered,
            answer: Some("x"),
        })
        .await?;
    assert_eq!(missing, SettleOutcome::NotFound);
    assert_eq!(
        f.handle.launch().open_questions_for(recipient).await?.len(),
        1
    );
    f.db.cleanup().await?;
    Ok(())
}
