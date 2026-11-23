use super::apply::{Applied, FileOutcome, edit_file_statement, retire_lesson};
use super::context::db;
use super::decide::{Action, Plan, Stmt, Verdict};
use super::group::{NewlyStated, group_statements};
use super::items::Env;
use super::llm::Failure;
use super::rewrite::Rewrite;
use super::record::{Entry, Recorder, text_before};
use super::subjects::Run;
use crate::store::persistence::consolidation::{DueSubject, NewDecision, StatementStateChange};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

pub struct Executed {
    pub decisions: Vec<NewDecision>,
    pub states: Vec<StatementStateChange>,
    pub regroup: Vec<i64>,
    pub rejudge: bool,
}

pub struct Accepted {
    pub merges: Vec<super::answer::MergeGroup>,
    pub rewrites: Vec<Rewrite>,
}

enum Op<'a> {
    Act(&'a Action),
    Rew(&'a Rewrite),
}

fn ops<'a>(plan: &'a Plan, rewrites: &'a [Rewrite], stmts: &HashMap<i64, Stmt>) -> Vec<Op<'a>> {
    let rewritten: std::collections::HashSet<i64> = rewrites.iter().map(|r| r.id).collect();
    let mut ops: Vec<Op<'a>> = plan
        .actions
        .iter()
        .filter(|a| !matches!(a, Action::Keep { id } if rewritten.contains(id)))
        .map(Op::Act)
        .chain(rewrites.iter().map(Op::Rew))
        .collect();
    let key = |op: &Op<'_>| {
        let id = match op {
            Op::Act(Action::Retire { loser, .. }) => *loser,
            Op::Rew(rewrite) => rewrite.id,
            Op::Act(_) => return None,
        };
        stmts
            .get(&id)
            .map(|s| (s.item_id, -i64::from(s.passage_start.unwrap_or(0))))
    };
    ops.sort_by_key(key);
    ops
}

pub async fn execute<L, E>(
    env: &Env<'_, L, E>,
    run: &Run,
    recorder: &Recorder,
    plan: &Plan,
    accepted: &Accepted,
    stmts: &HashMap<i64, Stmt>,
) -> Executed {
    let mut out = Executed {
        decisions: Vec::new(),
        states: Vec::new(),
        regroup: Vec::new(),
        rejudge: false,
    };
    for op in ops(plan, &accepted.rewrites, stmts) {
        match op {
            Op::Act(action) => execute_action(env, run, recorder, action, stmts, &mut out).await,
            Op::Rew(rewrite) => {
                super::rewrite::execute_rewrite(env, run, recorder, rewrite, stmts, &mut out).await;
            }
        }
    }
    super::merge::execute_merges(env, recorder, &accepted.merges, stmts, &mut out).await;
    out
}

async fn execute_action<L, E>(
    env: &Env<'_, L, E>,
    run: &Run,
    recorder: &Recorder,
    action: &Action,
    stmts: &HashMap<i64, Stmt>,
    out: &mut Executed,
) {
    match action {
        Action::Keep { id } => {
            let Some(stmt) = stmts.get(id) else { return };
            out.decisions.push(recorder.decision(
                stmt,
                Entry {
                    verdict: "kept",
                    counterpart: None,
                    before: stmt.text.clone(),
                    after: None,
                    reason: "Gives an instruction no other statement in the subject gives".to_owned(),
                    applied: Applied::Nothing,
                },
            ));
        }
        Action::Relate { id, counterpart, reason } => {
            push_plain(recorder, stmts, out, (*id, Some(*counterpart)), "kept", reason);
        }
        Action::OpenConflict { id, counterpart, reason } => {
            push_plain(recorder, stmts, out, (*id, Some(*counterpart)), "conflict_open", reason);
        }
        Action::Misgroup { id, reason } => {
            push_plain(recorder, stmts, out, (*id, None), "misgrouped", reason);
            out.regroup.push(*id);
        }
        Action::Retire { loser, by, verdict, reason } => {
            retire(env, run, recorder, (*loser, *by, *verdict, reason), stmts, out).await;
        }
    }
}

fn push_plain(
    recorder: &Recorder,
    stmts: &HashMap<i64, Stmt>,
    out: &mut Executed,
    (id, counterpart): (i64, Option<i64>),
    verdict: &str,
    reason: &str,
) {
    let Some(stmt) = stmts.get(&id) else { return };
    out.decisions.push(recorder.decision(
        stmt,
        Entry {
            verdict,
            counterpart: counterpart.and_then(|c| stmts.get(&c)),
            before: stmt.text.clone(),
            after: None,
            reason: reason.to_owned(),
            applied: Applied::Nothing,
        },
    ));
}

async fn retire<L, E>(
    env: &Env<'_, L, E>,
    run: &Run,
    recorder: &Recorder,
    (loser, by, verdict, reason): (i64, i64, Verdict, &str),
    stmts: &HashMap<i64, Stmt>,
    out: &mut Executed,
) {
    let (Some(stmt), Some(coverer)) = (stmts.get(&loser), stmts.get(&by)) else {
        return;
    };
    let outcome = if stmt.is_lesson() {
        FileOutcome::from(retire_lesson(env.persist, stmt, reason, Some(coverer)).await)
    } else if stmt.is_file() {
        edit_file_statement(env.persist, run.machine, env.settings.edit_files, stmt, None).await
    } else {
        FileOutcome::from(Applied::Nothing)
    };
    let applied = outcome.applied.clone();
    out.rejudge |= outcome.mismatch;
    if !matches!(applied, Applied::Failed(_)) {
        out.states.push(StatementStateChange {
            statement_id: stmt.id,
            state: verdict.code().to_owned(),
            resolved_by_statement_id: Some(coverer.id),
        });
    }
    let mut decision = recorder.decision(
        stmt,
        Entry {
            verdict: verdict.code(),
            counterpart: Some(coverer),
            before: text_before(stmt),
            after: stmt.is_file().then(String::new),
            reason: reason.to_owned(),
            applied,
        },
    );
    attach_edits(&mut decision, &outcome.edits);
    out.decisions.push(decision);
}

pub fn edit_items(edits: &[super::files::EditRecord]) -> Vec<serde_json::Value> {
    edits.iter().map(|edit| json!({"file_edit": edit})).collect()
}

pub fn attach_edits(decision: &mut NewDecision, edits: &[super::files::EditRecord]) {
    if let serde_json::Value::Array(items) = &mut decision.evidence {
        items.extend(edit_items(edits));
    }
}

pub async fn regroup<L, E>(
    env: &Env<'_, L, E>,
    subject: &DueSubject,
    plan: &Plan,
    misgrouped: &[i64],
) -> Result<(), Failure> {
    let knowledge = env.persist.knowledge();
    let judge_after = Utc::now() + env.settings.settle;
    for id in &plan.joins {
        knowledge
            .add_statement_to_subject(*id, subject.id, judge_after)
            .await
            .map_err(db)?;
    }
    if misgrouped.is_empty() {
        return Ok(());
    }
    let embeddings = knowledge.statement_embeddings(misgrouped).await.map_err(db)?;
    for (id, embedding) in embeddings {
        knowledge
            .remove_statement_from_subject(id, judge_after)
            .await
            .map_err(db)?;
        let fresh = [NewlyStated {
            statement_id: id,
            embedding,
        }];
        group_statements(
            env.persist,
            env.settings,
            subject.project_id,
            &fresh,
            Some(subject.id),
        )
        .await?;
    }
    Ok(())
}
