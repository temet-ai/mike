use super::answer::{PullKind, Relation, Universe, Validated, validate};
use super::context::{Context, db, load_context};
use super::decide::EvidenceItem;
use super::execute::{Accepted, execute, regroup};
use super::guard::Guard;
use super::items::Env;
use super::llm::{ConsolidationLlm, Embedder, Failure};
use super::prompt::{evidence_id, judge_user};
use super::record::Recorder;
use super::tally::Tally;
use crate::store::persistence::consolidation::DueSubject;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{NaiveTime, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Instant;
use uuid::Uuid;

pub struct Run {
    pub run_id: i64,
    pub machine: Option<Uuid>,
}

pub async fn process_subjects<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    run: &Run,
) -> Result<(), Failure> {
    let knowledge = env.persist.knowledge();
    let day_start = Utc::now().date_naive().and_time(NaiveTime::MIN).and_utc();
    let used = knowledge
        .consolidation_judgements_since(day_start)
        .await
        .map_err(db)?;
    let allowed = (env.settings.max_judgements_per_day - used).min(env.settings.subjects_per_tick);
    if allowed <= 0 {
        return Ok(());
    }
    for subject in knowledge
        .claim_due_knowledge_subjects(allowed)
        .await
        .map_err(db)?
    {
        if guard.open(Instant::now()) {
            break;
        }
        match judge_subject(env, guard, tally, run, &subject).await {
            Ok(()) => guard.forget(('s', subject.id)),
            Err(failure) => defer_subject(env, guard, tally, &subject, &failure).await,
        }
    }
    Ok(())
}

async fn defer_subject<L, E>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    subject: &DueSubject,
    failure: &Failure,
) {
    guard.failed(failure, Instant::now());
    let retry_at = guard.retry_at(('s', subject.id), failure, Utc::now());
    tally.error(format!("subject {}: {}", subject.id, failure.message));
    tracing::warn!(subject = subject.id, error = %failure.message, "knowledge consolidation: subject deferred");
    save_retry(env, subject.id, retry_at).await;
}

async fn save_retry<L, E>(env: &Env<'_, L, E>, subject_id: i64, retry_at: chrono::DateTime<Utc>) {
    let saved = env
        .persist
        .knowledge()
        .mark_knowledge_subjects_changed(&[subject_id], retry_at)
        .await;
    if let Err(error) = saved {
        tracing::warn!(%error, "knowledge consolidation: could not defer a subject");
    }
}

async fn judge_subject<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    run: &Run,
    subject: &DueSubject,
) -> Result<(), Failure> {
    let (context, _) = load_context(env, subject).await?;
    if context.members.is_empty() || context.skippable() {
        tally.subjects_skipped += 1;
        return finish(env, subject, None).await;
    }
    let user = judge_user(&context.members, &context.nearby, &context.evidence);
    let (answer, model) = match env.llm.judge(&user).await {
        Ok(answered) => {
            guard.succeeded(Instant::now());
            answered
        }
        Err(failure) => {
            guard.failed(&failure, Instant::now());
            return Err(failure);
        }
    };
    let member_ids: Vec<i64> = context.members.iter().map(|s| s.id).collect();
    let nearby_ids: Vec<i64> = context.nearby.iter().map(|s| s.id).collect();
    let evidence_ids: Vec<i64> = (0..context.evidence.len()).map(evidence_id).collect();
    let validated = validate(
        &answer,
        &Universe {
            members: &member_ids,
            nearby: &nearby_ids,
            evidence: &evidence_ids,
        },
    );
    tally.projects.insert(subject.project_id);
    let case = Case {
        run,
        subject,
        context: &context,
        validated: &validated,
        model: &model,
    };
    conclude(env, guard, tally, &case).await
}

struct Case<'a> {
    run: &'a Run,
    subject: &'a DueSubject,
    context: &'a Context,
    validated: &'a Validated,
    model: &'a str,
}

async fn finish<L, E>(
    env: &Env<'_, L, E>,
    subject: &DueSubject,
    concise: Option<(&str, &str)>,
) -> Result<(), Failure> {
    env.persist
        .knowledge()
        .finish_knowledge_subject_judgement(subject.id, subject.changed_at, concise)
        .await
        .map_err(db)?;
    Ok(())
}

fn evidence_json(context: &Context, validated: &Validated) -> Value {
    Value::Array(
        context
            .evidence
            .iter()
            .enumerate()
            .map(|(index, chunk)| {
                let used = validated
                    .evidence_used
                    .iter()
                    .find(|u| u.id == evidence_id(index));
                json!({
                    "reference": chunk.reference,
                    "kind": chunk.kind,
                    "similarity": chunk.similarity,
                    "excerpt": chunk.body.chars().take(200).collect::<String>(),
                    "dated_at": chunk.dated_at,
                    "used": used.is_some(),
                    "shows": used.map(|u| u.shows.clone()),
                })
            })
            .collect(),
    )
}

fn evidence_items(context: &Context, validated: &Validated) -> Vec<EvidenceItem> {
    validated
        .evidence_used
        .iter()
        .filter_map(|used| {
            let index = (used.id - evidence_id(0)) as usize;
            let chunk = context.evidence.get(index)?;
            Some(EvidenceItem {
                id: used.id,
                reference: chunk.reference.clone(),
                kind: chunk.kind.clone(),
                dated_at: chunk.dated_at,
            })
        })
        .collect()
}

fn tally_roles(tally: &mut Tally, project: Option<i64>, validated: &Validated) {
    let pulled: Vec<i64> = validated.pulled.iter().map(|p| p.id).collect();
    let roles = tally.roles.entry(project).or_default();
    roles.members += validated.members.len() as u32;
    for member in &validated.members {
        match member.relation {
            Relation::Elsewhere => roles.elsewhere += 1,
            Relation::CoveredBy(by) => {
                roles.covered += 1;
                roles.covered_outside += u32::from(pulled.contains(&by));
            }
            _ => {}
        }
    }
    for pull in &validated.pulled {
        if pull.kind == PullKind::CoveredByMember {
            roles.covered += 1;
            roles.covered_outside += 1;
        }
    }
}

async fn conclude<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    case: &Case<'_>,
) -> Result<(), Failure> {
    let Case {
        run,
        subject,
        context,
        validated,
        model,
    } = *case;
    let stmts: HashMap<i64, super::decide::Stmt> = context
        .members
        .iter()
        .chain(&context.nearby)
        .map(|s| (s.id, s.clone()))
        .collect();
    let plan = super::decide::plan(validated, &stmts, &evidence_items(context, validated));
    let accepted = Accepted {
        merges: super::merge::accept_merges(env, guard, &plan, &stmts).await,
        rewrites: super::merge::accept_rewrites(env, guard, &plan, &stmts).await,
    };
    let recorder = Recorder {
        run_id: run.run_id,
        project_id: subject.project_id,
        subject_id: subject.id,
        model: model.to_owned(),
        evidence: evidence_json(context, validated),
    };
    let executed = execute(env, run, &recorder, &plan, &accepted, &stmts).await;
    tally.decisions += executed.decisions.len() as u32;
    for decision in &executed.decisions {
        match decision.application.as_str() {
            "applied" => tally.applied += 1,
            "proposed" => tally.proposed += 1,
            "failed" => tally.failed += 1,
            _ => {}
        }
    }
    tally_roles(tally, subject.project_id, validated);
    let knowledge = env.persist.knowledge();
    knowledge
        .write_consolidation_decisions(&executed.decisions)
        .await
        .map_err(db)?;
    knowledge
        .set_statement_states(&executed.states)
        .await
        .map_err(db)?;
    let concise = (!validated.subject.is_empty() && !validated.text.is_empty())
        .then_some((validated.subject.as_str(), validated.text.as_str()));
    finish(env, subject, concise).await?;
    regroup(env, subject, &plan, &executed.regroup).await?;
    if executed.rejudge {
        env.persist
            .knowledge()
            .mark_knowledge_subjects_changed(&[subject.id], Utc::now() + env.settings.settle)
            .await
            .map_err(db)?;
    }
    tally.subjects_judged += 1;
    Ok(())
}

