use serde_json::json;
use uuid::Uuid;

use super::seed::{fixture, question, session};
use crate::store::persistence::questions::AskedQuestion;
use crate::store::persistence::session_notes::NewSessionNote;

#[tokio::test]
async fn a_question_about_a_note_is_listed_with_the_note_and_survives_its_deletion()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let author = session(&f.handle).await?;
    let asker = session(&f.handle).await?;
    let tags = vec!["fleet:f1".to_owned(), "task:T5".to_owned()];
    let note = f
        .handle
        .create_session_note(
            author,
            NewSessionNote {
                kind: Some("brief"),
                title: "Brief, task T5 (version 3)",
                body: Some("Your task"),
                status: None,
                tags: Some(tags.as_slice()),
                data: Some(&json!({"version": 3})),
            },
        )
        .await?;

    let reference = f.handle.note_ref(note.note_uuid).await?;
    let Some(reference) = reference else {
        anyhow::bail!("the note was not found");
    };
    assert_eq!(reference.author, author);
    assert_eq!(reference.title, "Brief, task T5 (version 3)");
    assert_eq!(reference.version, Some(3));
    assert_eq!(reference.tags, tags);
    assert!(f.handle.note_ref(Uuid::new_v4()).await?.is_none());

    let asked = f
        .handle
        .ask_question(&AskedQuestion {
            about_note: Some(note.note_uuid),
            ..question(asker, author, "is the file list right?")
        })
        .await?;
    let about = f.handle.questions_about_note(note.note_uuid).await?;
    assert_eq!(about.len(), 1);
    assert_eq!(about[0].question_uuid, asked.question_uuid);
    assert_eq!(about[0].about_note, Some(note.note_uuid));

    f.handle.delete_session_note(note.note_uuid).await?;
    let kept = f.handle.question_by_uuid(asked.question_uuid).await?;
    assert!(kept.is_some_and(|q| q.about_note.is_none()));
    assert!(
        f.handle
            .questions_about_note(note.note_uuid)
            .await?
            .is_empty()
    );
    f.db.cleanup().await?;
    Ok(())
}
