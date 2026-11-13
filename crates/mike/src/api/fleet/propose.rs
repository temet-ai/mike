use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::tree::running;
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::checks::normal;
use crate::launch::fleet::propose::{
    Answer, Proposal, answer_or_refusal, brief_for, next_task_id, propose, save_with,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub(crate) struct ProposeRequest {
    session: Uuid,
    #[serde(flatten)]
    proposal: Proposal,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct SplitRequest {
    pub(crate) session: Option<Uuid>,
    pub(crate) task: String,
    pub(crate) name: String,
    pub(crate) why: String,
    pub(crate) files: Vec<String>,
    pub(crate) criteria: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SplitAnswer {
    task: String,
    new_task: String,
    after: String,
    files: Vec<String>,
    criteria: Vec<String>,
    kept_files: Vec<String>,
    kept_criteria: Vec<String>,
}

pub(crate) async fn seat_of(
    persist: &crate::store::persistence::PersistHandle,
    run: &str,
    session: Uuid,
) -> Option<String> {
    let rows = persist.fleet().fleet_seat_rows(run).await.ok()?;
    rows.into_iter()
        .find(|row| row.session_uuid == Some(session))
        .map(|row| row.seat.unwrap_or_else(|| session.to_string()))
}

fn not_a_seat(session: Uuid) -> DefinitionError {
    DefinitionError::InvalidRequest(format!(
        "session {session} is not a seat of this fleet; pass your own session id as `session`"
    ))
}

pub(crate) async fn propose_task(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<ProposeRequest>,
) -> Result<Json<Answer>, DefinitionError> {
    let (row, _, run) = running(&state, &key).await?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    if seat_of(persist, &run, body.session).await.is_none() {
        return Err(not_a_seat(body.session));
    }
    let by = body.session.to_string();
    let answer = propose(persist, row.fleet_uuid, &by, &body.proposal, &[])
        .await
        .map_err(DefinitionError::Status)?;
    answer_or_refusal(answer)
        .map(Json)
        .map_err(DefinitionError::InvalidRequest)
}

fn listed_exactly(entries: &[String], wanted: &[String]) -> Result<(), String> {
    let mine: Vec<String> = entries.iter().map(|f| normal(f)).collect();
    let missing: Vec<&String> = wanted
        .iter()
        .filter(|w| !mine.contains(&normal(w)))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} {} not an entry of your task's file list; name entries exactly as listed: {}",
        missing
            .iter()
            .map(|m| format!("`{m}`"))
            .collect::<Vec<_>>()
            .join(", "),
        if missing.len() == 1 { "is" } else { "are" },
        mine.join(", ")
    ))
}

fn keep(entries: &[String], moved: &[String]) -> Vec<String> {
    let moved: Vec<String> = moved.iter().map(|m| normal(m)).collect();
    entries
        .iter()
        .filter(|e| !moved.contains(&normal(e)))
        .cloned()
        .collect()
}

fn split_problems(
    def: &FleetDefinition,
    session: Option<Uuid>,
    holder: Option<Uuid>,
    req: &SplitRequest,
) -> Result<(), String> {
    let task = def
        .tasks
        .get(&req.task)
        .ok_or_else(|| format!("`{}` is not a task of this fleet", req.task))?;
    if session.is_none() || session != holder {
        return Err(format!(
            "task {} is not held by this session; only the seat that holds a task may split it",
            req.task
        ));
    }
    if req.name.trim().is_empty() || req.files.is_empty() {
        return Err("name the new task (`name`) and the files it takes (`files`)".to_string());
    }
    listed_exactly(&task.files, &req.files)?;
    if let Some(c) = req.criteria.iter().find(|c| !task.criteria.contains(c)) {
        return Err(format!(
            "`{c}` is not a criterion of task {}; it serves {}",
            req.task,
            task.criteria.join(", ")
        ));
    }
    if keep(&task.files, &req.files).is_empty() {
        return Err(format!(
            "task {} would keep no file; leave it at least one",
            req.task
        ));
    }
    Ok(())
}

pub(crate) fn split_patch(
    def: &FleetDefinition,
    session: Option<Uuid>,
    holder: Option<Uuid>,
    req: &SplitRequest,
) -> Result<(Value, SplitAnswer), String> {
    split_problems(def, session, holder, req)?;
    let task = def.tasks.get(&req.task).ok_or("the task went away")?;
    let new_task = next_task_id(def);
    let moved_new: Vec<String> = task
        .new_files
        .iter()
        .filter(|f| req.files.iter().any(|r| normal(r) == normal(f)))
        .cloned()
        .collect();
    let criteria = if req.criteria.is_empty() {
        task.criteria.clone()
    } else {
        req.criteria.clone()
    };
    let kept_criteria = {
        let rest = keep(&task.criteria, &req.criteria);
        if rest.is_empty() {
            task.criteria.clone()
        } else {
            rest
        }
    };
    let proposal = Proposal {
        name: req.name.clone(),
        why: format!(
            "{}\n\nSplit from task {} ({}). Its brief, for context:\n{}",
            req.why.trim(),
            req.task,
            task.name,
            task.brief.trim()
        ),
        files: req.files.clone(),
        new_files: moved_new.clone(),
        criteria: criteria.clone(),
        after: vec![req.task.clone()],
        discipline: task.discipline.clone(),
    };
    let by = format!("the seat of task {}", req.task);
    let kept_files = keep(&task.files, &req.files);
    let mut tasks = Map::new();
    tasks.insert(
        req.task.clone(),
        json!({"files": kept_files, "new_files": keep(&task.new_files, &moved_new), "criteria": kept_criteria}),
    );
    tasks.insert(
        new_task.clone(),
        json!({"name": req.name.trim(), "discipline": task.discipline, "criteria": criteria,
               "files": req.files, "new_files": moved_new, "after": [req.task],
               "brief": brief_for(&proposal, &by)}),
    );
    let answer = SplitAnswer {
        task: req.task.clone(),
        new_task,
        after: req.task.clone(),
        files: req.files.clone(),
        criteria,
        kept_files,
        kept_criteria,
    };
    Ok((json!({ "tasks": tasks }), answer))
}

pub(crate) async fn split_task(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SplitRequest>,
) -> Result<Json<SplitAnswer>, DefinitionError> {
    let (row, _, run) = running(&state, &key).await?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let holder = crate::launch::fleet::definition::task_session(persist, &run, &body.task).await;
    let mut answer = None;
    let mut refusal = None;
    let saved = save_with(persist, row.fleet_uuid, |def| {
        match split_patch(def, body.session, holder, &body) {
            Ok((patch, built)) => {
                answer = Some(built);
                refusal = None;
                Ok((Some(patch), Answer::default()))
            }
            Err(why) => {
                refusal = Some(why);
                Ok((None, Answer::default()))
            }
        }
    })
    .await
    .map_err(DefinitionError::Status)?;
    if let Some(why) = refusal {
        return Err(DefinitionError::InvalidRequest(format!(
            "the task was not split: {why}"
        )));
    }
    match (saved, answer) {
        (Some(_), Some(answer)) => Ok(Json(answer)),
        _ => Err(DefinitionError::Changed(row.slug)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::{SplitRequest, split_patch};
    use crate::launch::fleet::{FleetDefinition, merge};

    const HOLDER: Uuid = Uuid::from_u128(7);

    fn def() -> FleetDefinition {
        let value = json!({
            "criteria": {"C1": {"text": "a", "check": "a"}, "C2": {"text": "b", "check": "b"}},
            "workers": {"rust-developer": {"model": "opus", "agent": "rust-developer"}},
            "tasks": {"T1": {"name": "both halves", "discipline": "rust-developer",
                             "criteria": ["C1", "C2"], "files": ["src/a.rs", "src/b.rs", "src/c/"],
                             "new_files": ["src/b.rs"], "brief": "Build a and b."}}
        });
        FleetDefinition::from_value(&value).unwrap_or_default()
    }

    fn ask(files: &[&str], criteria: &[&str]) -> SplitRequest {
        SplitRequest {
            session: Some(HOLDER),
            task: "T1".into(),
            name: "the b half".into(),
            why: "b is its own piece".into(),
            files: files.iter().map(|f| (*f).to_string()).collect(),
            criteria: criteria.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn a_split_moves_the_named_files_and_criteria_to_a_new_task_after_the_callers() {
        let def = def();
        let Ok((patch, answer)) = split_patch(
            &def,
            Some(HOLDER),
            Some(HOLDER),
            &ask(&["src/b.rs"], &["C2"]),
        ) else {
            panic!("a clean split is accepted");
        };
        assert_eq!(answer.new_task, "T2");
        let mut value = serde_json::to_value(&def).unwrap_or_default();
        merge(&mut value, &patch);
        let after = FleetDefinition::from_value(&value).unwrap_or_default();
        let old = &after.tasks["T1"];
        let new = &after.tasks["T2"];
        assert_eq!(old.files, vec!["src/a.rs", "src/c/"]);
        assert!(old.new_files.is_empty());
        assert_eq!(old.criteria, vec!["C1"]);
        assert_eq!(new.files, vec!["src/b.rs"]);
        assert_eq!(new.new_files, vec!["src/b.rs"]);
        assert_eq!(new.criteria, vec!["C2"]);
        assert_eq!(new.after, vec!["T1"]);
        assert_eq!(new.discipline, "rust-developer");
        assert!(new.brief.contains("b is its own piece") && new.brief.contains("Build a and b."));
        assert!(new.seat.is_none());
    }

    #[test]
    fn a_criterion_the_task_would_lose_entirely_stays_with_it_too() {
        let Ok((_, answer)) =
            split_patch(&def(), Some(HOLDER), Some(HOLDER), &ask(&["src/b.rs"], &[]))
        else {
            panic!("moving every criterion is allowed");
        };
        assert_eq!(answer.criteria, vec!["C1", "C2"]);
        assert_eq!(answer.kept_criteria, vec!["C1", "C2"]);
    }

    #[test]
    fn a_split_is_refused_for_another_seats_task_an_inexact_file_or_an_emptied_task() {
        let def = def();
        let other = split_patch(
            &def,
            Some(Uuid::from_u128(9)),
            Some(HOLDER),
            &ask(&["src/b.rs"], &[]),
        );
        assert!(other.is_err_and(|e| e.contains("not held by this session")));
        let inexact = split_patch(&def, Some(HOLDER), Some(HOLDER), &ask(&["src/c/d.rs"], &[]));
        assert!(inexact.is_err_and(|e| e.contains("`src/c/d.rs`") && e.contains("src/a.rs")));
        let foreign = split_patch(
            &def,
            Some(HOLDER),
            Some(HOLDER),
            &ask(&["src/b.rs"], &["C9"]),
        );
        assert!(foreign.is_err_and(|e| e.contains("`C9`")));
        let empty = split_patch(
            &def,
            Some(HOLDER),
            Some(HOLDER),
            &ask(&["src/a.rs", "src/b.rs", "src/c/"], &[]),
        );
        assert!(empty.is_err_and(|e| e.contains("would keep no file")));
    }

    #[test]
    fn a_split_is_refused_when_the_task_has_no_live_seat() {
        let refused = split_patch(&def(), Some(HOLDER), None, &ask(&["src/b.rs"], &[]));
        assert!(refused.is_err_and(|e| e.contains("not held by this session")));
    }
}
