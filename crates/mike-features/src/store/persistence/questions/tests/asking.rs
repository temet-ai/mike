use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::seed::{fixture, named, question, session};
use crate::store::persistence::PersistError;
use crate::store::persistence::questions::{AskedQuestion, STATE_OPEN};

#[tokio::test]
async fn a_question_is_open_for_its_recipient_oldest_first_with_who_asked() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let other = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;
    named(&f.handle, asker, "db-dev-T5").await?;

    let first = f
        .handle
        .launch()
        .ask_question(&AskedQuestion {
            blocked: true,
            ..question(asker, recipient, "which resolver?")
        })
        .await?;
    f.handle
        .launch()
        .ask_question(&question(other, recipient, "second"))
        .await?;
    f.handle
        .launch()
        .ask_question(&question(asker, other, "not for the recipient"))
        .await?;

    assert_eq!(first.state, STATE_OPEN);
    assert_eq!(first.asker, asker);
    assert_eq!(first.asker_name.as_deref(), Some("db-dev-T5"));
    assert!(first.blocked);
    assert!(first.settled_at.is_none() && first.escalated_at.is_none());

    let open = f.handle.launch().open_questions_for(recipient).await?;
    let texts: Vec<&str> = open.iter().map(|q| q.question.as_str()).collect();
    assert_eq!(texts, vec!["which resolver?", "second"]);
    assert_eq!(open[0].question_uuid, first.question_uuid);
    assert!(
        f.handle
            .launch()
            .open_questions_for(asker)
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_question_to_a_task_with_no_seat_yet_keeps_the_task_name() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let asked = f
        .handle
        .launch()
        .ask_question(&AskedQuestion {
            asker,
            to_session: None,
            to_task: Some("T9"),
            run_id: None,
            about_note: None,
            question: "who has T9?",
            blocked: false,
        })
        .await?;
    assert_eq!(asked.to_task.as_deref(), Some("T9"));
    assert!(asked.to_session.is_none());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn asking_refuses_a_blank_question_an_unknown_asker_and_an_unknown_recipient()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let asker = session(&f.handle).await?;
    let recipient = session(&f.handle).await?;

    let blank = f
        .handle
        .launch()
        .ask_question(&question(asker, recipient, "  "))
        .await;
    assert!(matches!(&blank, Err(PersistError::DataInvariant(m)) if m.contains("empty")));

    let stranger = Uuid::new_v4();
    let unknown_asker = f
        .handle
        .launch()
        .ask_question(&question(stranger, recipient, "hello"))
        .await;
    assert!(matches!(&unknown_asker, Err(PersistError::DataInvariant(m)) if m.contains("asker")));

    let unknown_recipient = f
        .handle
        .launch()
        .ask_question(&question(asker, stranger, "hello"))
        .await;
    assert!(
        matches!(&unknown_recipient, Err(PersistError::DataInvariant(m)) if m.contains("recipient"))
    );

    let nobody = f
        .handle
        .launch()
        .ask_question(&AskedQuestion {
            to_session: None,
            ..question(asker, recipient, "hello")
        })
        .await;
    assert!(nobody.is_err());
    assert!(
        f.handle
            .launch()
            .open_questions_for(recipient)
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}
