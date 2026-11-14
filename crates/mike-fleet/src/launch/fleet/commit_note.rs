use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::commit::CommitReport;
use super::definition::FleetDefinition;
use super::plan::ControlSeats;
use super::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NewSessionNote, NoteOrder, SessionNoteFilter};

const VERDICTS_READ: i64 = 200;

const KIND: &str = "result";
const SHA_SHOWN: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    pub added: Option<u64>,
    pub deleted: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub data: Value,
}

pub fn parse_numstat(raw: &str) -> Vec<Changed> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (added, deleted, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some(Changed {
                path: path.to_string(),
                added: added.parse().ok(),
                deleted: deleted.parse().ok(),
            })
        })
        .collect()
}

fn changed_line(file: &Changed) -> String {
    match (file.added, file.deleted) {
        (Some(added), Some(deleted)) => format!("- {} (+{added} -{deleted})", file.path),
        _ => format!("- {} (not text)", file.path),
    }
}

fn files_part(changed: &[Changed]) -> String {
    if changed.is_empty() {
        return "No file changed; the commit carries its message only.".to_string();
    }
    let lines: Vec<String> = changed.iter().map(changed_line).collect();
    format!("Files changed:\n{}", lines.join("\n"))
}

pub fn draft(report: &CommitReport, message: &str, changed: &[Changed]) -> Option<Draft> {
    let sha = report.sha.as_deref()?;
    let mut lines = message.trim().lines();
    let subject = lines.next().unwrap_or_default().trim();
    let said = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    let mut parts = Vec::new();
    if !said.is_empty() {
        parts.push(said);
    }
    parts.push(files_part(changed));
    parts.push(format!("How it was verified: {}", report.verified));
    if !report.warnings.is_empty() {
        let listed: Vec<String> = report.warnings.iter().map(|w| format!("- {w}")).collect();
        parts.push(format!(
            "What mike warned about at the commit:\n{}",
            listed.join("\n")
        ));
    }
    Some(Draft {
        title: format!(
            "{} committed {}: {subject}",
            report.task,
            &sha[..sha.len().min(SHA_SHOWN)]
        ),
        body: parts.join("\n\n"),
        tags: vec!["commit".to_string(), report.task.clone()],
        data: json!({
            "task": report.task,
            "seat": report.seat,
            "sha": sha,
            "paths": report.paths,
            "changed": changed
                .iter()
                .map(|c| json!({"path": c.path, "added": c.added, "deleted": c.deleted}))
                .collect::<Vec<_>>(),
            "verified": report.verified,
            "warnings": report.warnings,
        }),
    })
}

pub fn names_task(data: &Value, task: &str) -> bool {
    ["task", "package"]
        .into_iter()
        .filter_map(|key| data.get(key).and_then(Value::as_str))
        .chain(
            ["tasks", "packages"]
                .into_iter()
                .filter_map(|key| data.get(key).and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str),
        )
        .any(|named| named == task)
}

pub fn holds_verdict(data: &Value, task: &str) -> bool {
    matches!(
        data.get("verdict")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .as_deref(),
        Some("pass" | "fail")
    ) && names_task(data, task)
}

pub fn acceptance_mail(report: &CommitReport) -> Option<String> {
    let sha = report.sha.as_deref()?;
    Some(format!(
        "[mike] {} committed {} for task {} after your verdict on it: {}. Files: {}. Verified: {}. Judge it, or say it needs nothing.",
        report.seat,
        &sha[..sha.len().min(SHA_SHOWN)],
        report.task,
        sha,
        report.paths.join(", "),
        report.verified
    ))
}

async fn verdict_recorded(persist: &PersistHandle, acceptance: Uuid, task: &str) -> bool {
    let filter = SessionNoteFilter {
        kind: Some("decision"),
        status: None,
        tag: None,
        q: None,
        order: NoteOrder::NewestFirst,
        limit: VERDICTS_READ,
        offset: 0,
    };
    match persist
        .launch()
        .fetch_session_notes(acceptance, filter)
        .await
    {
        Ok(notes) => notes.iter().any(|n| holds_verdict(&n.data, task)),
        Err(err) => {
            tracing::warn!(error = %err, "fleet: could not read the acceptance seat's verdicts");
            false
        }
    }
}

pub async fn tell_acceptance(
    persist: &PersistHandle,
    def: &FleetDefinition,
    report: &CommitReport,
) {
    let Some(acceptance) = ControlSeats::recorded(def).acceptance else {
        return;
    };
    let Some(body) = acceptance_mail(report) else {
        return;
    };
    if !verdict_recorded(persist, acceptance, &report.task).await {
        return;
    }
    if let Err(err) = persist
        .launch()
        .insert_message(Uuid::new_v4(), acceptance, None, &body)
        .await
    {
        tracing::warn!(error = %err, "fleet: could not tell the acceptance seat about a commit after its verdict");
    }
}

pub async fn write(persist: &PersistHandle, session: Uuid, message: &str, report: &CommitReport) {
    let Some(sha) = report.sha.as_deref() else {
        return;
    };
    let numstat = git(&report.repo, &["show", "--numstat", "--format=", sha], None)
        .await
        .map(|raw| String::from_utf8_lossy(&raw).into_owned())
        .unwrap_or_default();
    let Some(note) = draft(report, message, &parse_numstat(&numstat)) else {
        return;
    };
    store(persist, session, &note).await;
}

pub async fn store(persist: &PersistHandle, session: Uuid, note: &Draft) {
    let new = NewSessionNote {
        kind: Some(KIND),
        title: &note.title,
        body: Some(&note.body),
        status: None,
        tags: Some(&note.tags),
        data: Some(&note.data),
    };
    if let Err(err) = persist.fleet().create_session_note(session, new).await {
        tracing::warn!(session_id = %session, error = %err, "fleet: could not write a result note for a seat");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn landed() -> CommitReport {
        CommitReport {
            seat: "sim-developer-T04".into(),
            task: "T04".into(),
            paths: vec!["crates/sim/src/air/mod.rs".into(), "assets/tile.png".into()],
            sha: Some("0123456789abcdef0123".into()),
            verified: "proven: the check passed at state abc, the content of this commit under the check's paths".into(),
            warnings: vec!["`rustfmt --check` does not pass on the files just committed".into()],
            ..CommitReport::default()
        }
    }

    #[test]
    fn the_note_says_what_changed_where_and_how_it_was_verified() {
        let changed = parse_numstat("12\t3\tcrates/sim/src/air/mod.rs\n-\t-\tassets/tile.png\n");
        assert_eq!(changed.len(), 2);
        assert_eq!((changed[0].added, changed[0].deleted), (Some(12), Some(3)));
        let message = "feat(air): pressure falls with height\n\nA column loses a tenth per level, so a sealed room on a hill holds less air.";
        let Some(note) = draft(&landed(), message, &changed) else {
            panic!("a landed commit has a note");
        };
        assert_eq!(
            note.title,
            "T04 committed 0123456789: feat(air): pressure falls with height"
        );
        assert_eq!(
            note.body,
            "A column loses a tenth per level, so a sealed room on a hill holds less air.\n\nFiles changed:\n- crates/sim/src/air/mod.rs (+12 -3)\n- assets/tile.png (not text)\n\nHow it was verified: proven: the check passed at state abc, the content of this commit under the check's paths\n\nWhat mike warned about at the commit:\n- `rustfmt --check` does not pass on the files just committed"
        );
        assert_eq!(note.tags, vec!["commit".to_string(), "T04".to_string()]);
        assert_eq!(note.data["sha"], json!("0123456789abcdef0123"));
        assert_eq!(note.data["changed"][0]["added"], json!(12));
    }

    #[test]
    fn nothing_is_written_for_a_commit_that_did_not_land() {
        let refused = CommitReport {
            sha: None,
            refused: Some("nothing to commit".into()),
            ..landed()
        };
        assert_eq!(draft(&refused, "x", &[]), None);
        let Some(empty) = draft(&landed(), "chore: close the task\n", &[]) else {
            panic!("an empty commit that landed has a note");
        };
        assert!(
            empty
                .body
                .starts_with("No file changed; the commit carries its message only.")
        );
    }

    #[test]
    fn a_verdict_naming_the_task_is_found_and_one_for_another_task_is_not() {
        let pass = json!({"verdict": "PASS", "tasks": ["T5", "T2"]});
        assert!(holds_verdict(&pass, "T2"));
        assert!(!holds_verdict(&pass, "T3"));
        assert!(holds_verdict(
            &json!({"verdict": "fail", "task": "T2"}),
            "T2"
        ));
        assert!(!holds_verdict(&json!({"task": "T2"}), "T2"));
        assert!(!holds_verdict(
            &json!({"verdict": "note", "task": "T2"}),
            "T2"
        ));
    }

    #[test]
    fn the_mail_to_acceptance_names_the_commit_the_task_and_the_files() {
        let report = CommitReport {
            seat: "rust-developer-T2".into(),
            task: "T2".into(),
            paths: vec!["a.rs".into(), "b.rs".into()],
            sha: Some("0123456789abcdef".into()),
            verified: "heuristic".into(),
            ..CommitReport::default()
        };
        let mail = acceptance_mail(&report).unwrap_or_default();
        assert!(
            mail.contains("0123456789 for task T2") && mail.contains("a.rs, b.rs"),
            "{mail}"
        );
        assert_eq!(acceptance_mail(&CommitReport::default()), None);
    }
}
