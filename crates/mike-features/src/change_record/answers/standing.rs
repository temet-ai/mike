use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::caller::{AnswerError, Caller};
use super::lines::{ChangeLine, line_of, names_for};
use crate::change_record::{ChangeKind, ChangeQuery, OpenChange};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::questions::{NoteRef, QuestionRow};

const WAITING_COUNT_CAP: i64 = 200;
const NEWEST_COMMITS: i64 = 5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OtherSession {
    pub who: String,
    pub uncommitted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StandingAnswer {
    pub uncommitted: Vec<String>,
    pub events_waiting: i64,
    pub events_waiting_capped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub others: Option<Vec<OtherSession>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_commits: Option<Vec<ChangeLine>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<QuestionLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionAbout {
    pub note: Uuid,
    pub title: String,
    pub version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionLine {
    pub question_id: Uuid,
    pub from: String,
    pub blocked: bool,
    pub asked_at: DateTime<Utc>,
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<QuestionAbout>,
}

pub fn question_line(row: QuestionRow, about: Option<NoteRef>) -> QuestionLine {
    let from = row
        .asker_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| row.asker.to_string());
    QuestionLine {
        question_id: row.question_uuid,
        from,
        blocked: row.blocked,
        asked_at: row.created_at,
        question: row.question,
        about: about.map(|note| QuestionAbout {
            note: note.note_uuid,
            title: note.title,
            version: note.version,
        }),
    }
}

async fn open_questions(
    persist: &PersistHandle,
    caller: &Caller,
) -> Result<Vec<QuestionLine>, AnswerError> {
    let identities = persist.session_identities(&[caller.session_id]).await?;
    let Some(me) = identities.first() else {
        return Ok(Vec::new());
    };
    let rows = persist.launch().open_questions_for(me.uuid).await?;
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        let about = match row.about_note {
            Some(note) => persist.launch().note_ref(note).await?,
            None => None,
        };
        lines.push(question_line(row, about));
    }
    Ok(lines)
}

pub async fn standing(
    persist: &PersistHandle,
    caller: &Caller,
) -> Result<StandingAnswer, AnswerError> {
    let open = persist.fleet().open_changes(&caller.checkout).await?;
    let uncommitted = paths_of(&open, Some(caller.session_id));
    let (events_waiting, events_waiting_capped) = waiting(persist, caller).await?;
    let (others, newest_commits) = if caller.in_fleet {
        (None, None)
    } else {
        (
            Some(others(persist, caller, &open).await?),
            Some(newest_commits(persist, caller).await?),
        )
    };
    let questions = open_questions(persist, caller).await?;
    Ok(StandingAnswer {
        uncommitted,
        events_waiting,
        events_waiting_capped,
        others,
        newest_commits,
        questions,
    })
}

fn paths_of(open: &[OpenChange], session: Option<i64>) -> Vec<String> {
    let mut paths: Vec<String> = open
        .iter()
        .filter(|o| o.session_id == session)
        .map(|o| o.path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

async fn waiting(persist: &PersistHandle, caller: &Caller) -> Result<(i64, bool), AnswerError> {
    let place = persist
        .fleet()
        .change_place(caller.session_id, &caller.checkout)
        .await?;
    let query = ChangeQuery {
        exclude_session: Some(caller.session_id),
        ..ChangeQuery::everything_after(&caller.checkout, place, WAITING_COUNT_CAP + 1)
    };
    let count = persist.fleet().change_events_after(&query).await?.len() as i64;
    Ok((count.min(WAITING_COUNT_CAP), count > WAITING_COUNT_CAP))
}

async fn others(
    persist: &PersistHandle,
    caller: &Caller,
    open: &[OpenChange],
) -> Result<Vec<OtherSession>, AnswerError> {
    let mut by_session: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for change in open {
        if let Some(id) = change.session_id.filter(|id| *id != caller.session_id) {
            by_session.entry(id).or_default().push(change.path.clone());
        }
    }
    let ids: Vec<i64> = by_session.keys().copied().collect();
    let identities = if ids.is_empty() {
        Vec::new()
    } else {
        persist.session_identities(&ids).await?
    };
    Ok(by_session
        .into_iter()
        .map(|(id, mut uncommitted)| {
            uncommitted.sort();
            uncommitted.dedup();
            let who = identities
                .iter()
                .find(|i| i.id == id)
                .map(|i| i.name.clone().unwrap_or_else(|| i.uuid.to_string()))
                .unwrap_or_else(|| format!("session {id}"));
            OtherSession { who, uncommitted }
        })
        .collect())
}

async fn newest_commits(
    persist: &PersistHandle,
    caller: &Caller,
) -> Result<Vec<ChangeLine>, AnswerError> {
    let commits = persist
        .fleet()
        .latest_change_events(&caller.checkout, &[], &[ChangeKind::Commit], NEWEST_COMMITS)
        .await?;
    let names = names_for(persist, &commits).await?;
    Ok(commits.iter().map(|e| line_of(e, &names)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn open(path: &str, session: Option<i64>) -> OpenChange {
        OpenChange {
            path: path.to_owned(),
            session_id: session,
            at: Utc::now(),
        }
    }

    fn row(asker_name: Option<&str>, blocked: bool) -> QuestionRow {
        QuestionRow {
            question_uuid: Uuid::from_u128(1),
            run_id: None,
            asker: Uuid::from_u128(2),
            asker_name: asker_name.map(str::to_owned),
            to_session: Some(Uuid::from_u128(3)),
            to_task: None,
            about_note: None,
            question: "which file?".to_owned(),
            blocked,
            state: "open".to_owned(),
            answer: None,
            settled_by: None,
            created_at: Utc::now(),
            settled_at: None,
            escalated_at: None,
        }
    }

    #[test]
    fn a_question_line_names_the_asker_and_whether_they_wait() {
        let named = question_line(row(Some("db-dev-T5"), true), None);
        assert_eq!(named.from, "db-dev-T5");
        assert!(named.blocked);
        assert!(named.about.is_none());
        let unnamed = question_line(row(Some("  "), false), None);
        assert_eq!(unnamed.from, Uuid::from_u128(2).to_string());
    }

    #[test]
    fn a_question_about_a_brief_note_carries_the_notes_title_and_version() {
        let note = NoteRef {
            note_uuid: Uuid::from_u128(9),
            author: Uuid::from_u128(4),
            title: "Brief, task T5 (version 3)".to_owned(),
            body: String::new(),
            tags: Vec::new(),
            version: Some(3),
        };
        let line = question_line(row(None, false), Some(note));
        let about = line.about.map(|a| (a.title, a.version));
        assert_eq!(
            about,
            Some(("Brief, task T5 (version 3)".to_owned(), Some(3)))
        );
    }

    #[tokio::test]
    async fn standing_lists_the_callers_open_questions_oldest_first_and_drops_settled_ones()
    -> anyhow::Result<()> {
        use crate::store::persistence::change_record::test_seed::{fixture, project, session};
        use crate::store::persistence::questions::{AskedQuestion, SettleKind, SettledQuestion};
        let Some(f) = fixture().await? else {
            return Ok(());
        };
        let pool = f.handle.pool();
        let project_id = project(pool).await?;
        let me = session(pool, project_id, None).await?;
        let peer = session(pool, project_id, None).await?;
        let identities = f.handle.session_identities(&[me, peer]).await?;
        let uuid_of = |id: i64| identities.iter().find(|i| i.id == id).map(|i| i.uuid);
        let (Some(me_uuid), Some(peer_uuid)) = (uuid_of(me), uuid_of(peer)) else {
            anyhow::bail!("the seeded sessions have no identity");
        };
        let ask = |text: &'static str, blocked: bool| AskedQuestion {
            asker: peer_uuid,
            to_session: Some(me_uuid),
            to_task: None,
            run_id: None,
            about_note: None,
            question: text,
            blocked,
        };
        let first = f.handle.launch().ask_question(&ask("first", true)).await?;
        f.handle
            .launch()
            .ask_question(&ask("second", false))
            .await?;
        let settled = f
            .handle
            .launch()
            .ask_question(&ask("settled", false))
            .await?;
        f.handle
            .launch()
            .settle_question(&SettledQuestion {
                question: settled.question_uuid,
                by: me_uuid,
                kind: SettleKind::Declined,
                answer: None,
            })
            .await?;
        let caller = Caller {
            session_id: me,
            checkout: "/work/alpha".to_owned(),
            in_fleet: true,
            task_files: Vec::new(),
        };
        let answer = standing(&f.handle, &caller).await?;
        let texts: Vec<&str> = answer
            .questions
            .iter()
            .map(|q| q.question.as_str())
            .collect();
        assert_eq!(texts, vec!["first", "second"]);
        assert_eq!(answer.questions[0].question_id, first.question_uuid);
        assert!(answer.questions[0].blocked);
        let other = Caller {
            session_id: peer,
            ..caller
        };
        assert!(standing(&f.handle, &other).await?.questions.is_empty());
        f.db.cleanup().await?;
        Ok(())
    }

    #[test]
    fn a_session_sees_only_its_own_open_paths_sorted_and_unique() {
        let changes = vec![
            open("b.rs", Some(1)),
            open("a.rs", Some(1)),
            open("a.rs", Some(1)),
            open("c.rs", Some(2)),
        ];
        assert_eq!(
            paths_of(&changes, Some(1)),
            vec!["a.rs".to_owned(), "b.rs".to_owned()]
        );
    }
}
