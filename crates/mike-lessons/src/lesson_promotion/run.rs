use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::budget::cap_from_setting;
use super::checkout::eligible_checkout;
use super::cleanup::{CleanupReport, clean_up};
use super::drafts::DraftSets;
use super::ledger::{Ledger, Measured, shipped_skills};
use super::model::{LessonRow, STANDING_FILE};
use super::owned::{Conflicts, Manifest, add_exclude_lines, exclude_file, reconcile};
use super::plan::{PlanInputs, SkillPlan, group_by_project, plan_skills};
use super::publish::{Published, publish_outcomes, report_installs};
use super::routing::{CoverageSettings, Routing, RoutingInputs, route_lessons};
use super::settle::{SettleInputs, Settled, settle};
use super::state::{Environment, load_json, load_recipes, store_json};
use super::store;
use crate::lesson_worker::LessonRoute;
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct PromotionReport {
    pub result: Value,
    pub has_conflicts: bool,
}

struct Facts {
    lessons: Vec<LessonRow>,
    drafts: DraftSets,
    policies: BTreeMap<String, String>,
    user_retired: BTreeSet<String>,
    locations: BTreeMap<String, Vec<String>>,
    loads: BTreeMap<String, i64>,
    cap: usize,
    existing: BTreeMap<i64, LessonRoute>,
}

async fn read_facts(pool: &PgPool, machine: Uuid) -> Result<Facts> {
    Ok(Facts {
        lessons: store::accepted_lessons(pool, machine).await?,
        drafts: DraftSets::from_rows(store::drafts(pool).await?),
        policies: store::policies(pool).await?,
        user_retired: store::user_retired(pool).await?,
        locations: store::locations(pool, machine).await?,
        loads: store::skill_loads(pool).await?,
        cap: cap_from_setting(store::skill_cap_setting(pool).await?.as_ref()),
        existing: store::existing_routes(pool).await?,
    })
}

fn injection_lessons(lessons: &[LessonRow], routes: &BTreeMap<i64, LessonRoute>) -> Vec<LessonRow> {
    lessons
        .iter()
        .filter(|row| routes.get(&row.id).is_none_or(|route| route.is_injection()))
        .cloned()
        .collect()
}

fn collect_excludes(
    plan: &SkillPlan,
    routing: &Routing,
    conflicts: &Conflicts,
) -> BTreeMap<PathBuf, Vec<String>> {
    let mut excludes = plan.excludes.clone();
    for emission in plan.emitted.values() {
        if !conflicts.contains_key(&emission.path.display().to_string()) {
            excludes
                .entry(emission.checkout.clone())
                .or_default()
                .push(format!(".claude/skills/{}/", emission.folder));
        }
    }
    for (path, checkout) in &routing.standing_checkouts {
        if !conflicts.contains_key(&path.display().to_string()) {
            excludes
                .entry(checkout.clone())
                .or_default()
                .push(STANDING_FILE.to_owned());
        }
    }
    excludes
}

fn apply_excludes(excludes: &BTreeMap<PathBuf, Vec<String>>, apply: bool) -> Vec<String> {
    let mut skipped = Vec::new();
    for (checkout, lines) in excludes {
        match exclude_file(checkout) {
            None => skipped.push(checkout.display().to_string()),
            Some(target) if apply => {
                if add_exclude_lines(&target, lines).is_err() {
                    skipped.push(checkout.display().to_string());
                }
            }
            Some(_) => {}
        }
    }
    skipped
}

struct Summary<'a> {
    facts: &'a Facts,
    plan: &'a SkillPlan,
    settled: &'a Settled,
    measured: &'a Measured,
    conflicts: &'a Conflicts,
    routing: &'a Routing,
    cleanup: &'a CleanupReport,
    published: &'a Published,
    install_report: Option<Value>,
    exclude_skipped: Vec<String>,
    routes_saved: u64,
    dry_run: bool,
}

fn route_counts(routing: &Routing) -> Value {
    let count = |wanted: LessonRoute| {
        routing
            .decisions
            .iter()
            .filter(|decision| decision.route == wanted)
            .count()
    };
    json!({
        "standing": count(LessonRoute::Standing),
        "covered": count(LessonRoute::Covered),
        "injection": count(LessonRoute::Injection),
        "changed": 0,
    })
}

fn result_json(summary: &Summary<'_>) -> Value {
    let (plan, settled) = (summary.plan, summary.settled);
    let checkouts: BTreeSet<&PathBuf> = plan.emitted.values().map(|e| &e.checkout).collect();
    let skills: Vec<&String> = plan
        .desired
        .keys()
        .filter(|path| path.ends_with("/SKILL.md"))
        .collect();
    let mut routes = route_counts(summary.routing);
    routes["changed"] = json!(summary.routes_saved);
    json!({
        "dry_run": summary.dry_run,
        "lessons": summary.facts.lessons.len(),
        "memory_projects": group_by_project(&summary.facts.lessons).len(),
        "held_by_user": plan.held_by_user,
        "held_for_approval": plan.held_for_approval,
        "install_report": summary.install_report,
        "skills": skills,
        "conflicts": summary.conflicts,
        "topic_skills": plan.emitted.len(),
        "topic_skill_projects": checkouts.len(),
        "authored_skills": settled.authored,
        "skipped_projects": plan.skipped,
        "exclude_skipped": summary.exclude_skipped,
        "retired": settled.retired,
        "retired_corrected_above_base": settled.harmful,
        "held_retired": plan.held_retired,
        "skill_cap": summary.facts.cap,
        "capped_retired": settled.capped,
        "held_for_cap": settled.held_for_cap,
        "purpose_held": plan.purpose_held,
        "live_skills": summary.measured.len(),
        "live_outcomes_sent": summary.published.sent,
        "live_publish_error": summary.published.error,
        "routes": routes,
        "standing_files": summary.routing.standing.len(),
        "coverage": summary.routing.coverage,
        "cleanup": summary.cleanup,
    })
}

struct Decided {
    routing: Routing,
    plan: SkillPlan,
    routes_saved: u64,
}

async fn decide(
    persist: &PersistHandle,
    coverage: Option<&CoverageSettings<'_>>,
    environment: &Environment,
    facts: &Facts,
    ledger: &mut Ledger,
    apply: bool,
) -> Result<Decided> {
    let today = chrono::Local::now().date_naive();
    let home = environment.home.clone();
    let eligible = move |path: &Path| eligible_checkout(path, &home);
    let routing = route_lessons(
        persist,
        &RoutingInputs {
            lessons: &facts.lessons,
            locations: &facts.locations,
            existing: facts.existing.clone(),
            eligible: &eligible,
            coverage,
        },
    )
    .await;
    let routes_saved = store::save_routes(&persist.pool, &routing.decisions, apply).await?;
    let skill_lessons = injection_lessons(&facts.lessons, &routing.routes);
    let recipes = load_recipes(&environment.recipes_dir());
    let (roots, runner) = (environment.skill_roots(), environment.runner());
    let mut plan = plan_skills(
        &PlanInputs {
            today,
            lessons: &skill_lessons,
            drafts: &facts.drafts,
            policies: &facts.policies,
            user_retired: &facts.user_retired,
            locations: &facts.locations,
            recipes: &recipes,
            skill_roots: &roots,
            runner: &runner,
            eligible: &eligible,
        },
        ledger,
    );
    for (path, body) in &routing.standing {
        plan.desired
            .insert(path.display().to_string(), body.clone());
    }
    Ok(Decided {
        routing,
        plan,
        routes_saved,
    })
}

pub async fn promote(
    persist: &PersistHandle,
    coverage: Option<&CoverageSettings<'_>>,
    environment: &Environment,
    options: &Options,
) -> Result<PromotionReport> {
    let apply = !options.dry_run;
    let today = chrono::Local::now().date_naive();
    let state = environment.state_dir();
    let mut manifest: Manifest = load_json(&state.join("manifest.json"))?;
    let mut ledger: Ledger = load_json(&state.join("skill-installs.json"))?;
    let pool = &persist.pool;
    let cleanup = clean_up(&environment.data_root(), apply);
    let facts = read_facts(pool, environment.machine_uuid).await?;
    let Decided {
        routing,
        mut plan,
        routes_saved,
    } = decide(persist, coverage, environment, &facts, &mut ledger, apply).await?;
    let keys: Vec<String> = plan.emitted.keys().cloned().collect();
    let measured = store::measure(pool, &shipped_skills(&ledger, &keys)).await?;
    let settled = settle(
        &mut plan,
        &mut ledger,
        &SettleInputs {
            today,
            loads: &facts.loads,
            cap: facts.cap,
            measured: &measured,
        },
    );
    let conflicts = reconcile(&plan.desired, &mut manifest, &state, apply)?;
    let exclude_skipped = apply_excludes(&collect_excludes(&plan, &routing, &conflicts), apply);
    let published = if apply {
        publish_outcomes(persist, &mut ledger).await
    } else {
        Published {
            sent: 0,
            error: None,
        }
    };
    let install_report = if apply {
        let machine = (environment.machine_uuid, environment.machine_name.as_str());
        Some(report_installs(persist, machine, &ledger).await)
    } else {
        None
    };
    let result = result_json(&Summary {
        facts: &facts,
        plan: &plan,
        settled: &settled,
        measured: &measured,
        conflicts: &conflicts,
        routing: &routing,
        cleanup: &cleanup,
        published: &published,
        install_report,
        exclude_skipped,
        routes_saved,
        dry_run: !apply,
    });
    if apply {
        store_json(&state.join("skill-installs.json"), &ledger)?;
        store_json(&state.join("manifest.json"), &manifest)?;
        store_json(&state.join("latest.json"), &result)?;
    }
    Ok(PromotionReport {
        result,
        has_conflicts: !conflicts.is_empty(),
    })
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
