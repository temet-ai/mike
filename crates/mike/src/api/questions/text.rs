use uuid::Uuid;

pub(super) use crate::launch::fleet::questions::who;
use crate::store::persistence::questions::QuestionRow;

pub(super) fn answered_mail(row: &QuestionRow, by: &str, amended: Option<(Uuid, i64)>) -> String {
    let amendment = amended
        .map(|(note, version)| {
            format!("\nThe answer was added to the brief: version {version}, note {note}.")
        })
        .unwrap_or_default();
    format!(
        "[question {id}] {by} answered your question \"{question}\":\n{answer}{amendment}",
        id = row.question_uuid,
        question = row.question,
        answer = row.answer.as_deref().unwrap_or_default(),
    )
}

pub(super) fn declined_mail(row: &QuestionRow, by: &str) -> String {
    let reason = row
        .answer
        .as_deref()
        .map(|r| format!(" Reason: {r}"))
        .unwrap_or_default();
    format!(
        "[question {id}] {by} declined your question \"{question}\".{reason}",
        id = row.question_uuid,
        question = row.question,
    )
}

pub(super) fn amendment_entry(row: &QuestionRow, by: &str) -> String {
    format!(
        "Question {id} from {asker}: {question}\nAnswer from {by}: {answer}",
        id = row.question_uuid,
        asker = who(row.asker_name.as_deref(), row.asker),
        question = row.question,
        answer = row.answer.as_deref().unwrap_or_default(),
    )
}
