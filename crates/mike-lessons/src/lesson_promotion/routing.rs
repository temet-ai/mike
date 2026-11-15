use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::checkout::{Eligible, local_checkout};
use super::coverage::{Embedded, Unit, covered_lessons, hand_written_units};
use super::model::{LessonRow, STANDING_FILE};
use super::plan::group_by_project;
use super::render::standing_body;
use super::routes::{RouteDecision, decide_routes, standing_rows};
use crate::analysis::EmbeddingService;
use crate::lesson_worker::LessonRoute;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

const EMBED_BATCH: usize = 64;

pub struct CoverageSettings<'a> {
    pub service: &'a EmbeddingService,
    pub threshold: f64,
    pub allow_remote: bool,
}

pub(super) struct RoutingInputs<'a> {
    pub lessons: &'a [LessonRow],
    pub locations: &'a BTreeMap<String, Vec<String>>,
    pub existing: BTreeMap<i64, LessonRoute>,
    pub eligible: Eligible<'a>,
    pub coverage: Option<&'a CoverageSettings<'a>>,
}

pub(super) struct Routing {
    pub routes: BTreeMap<i64, LessonRoute>,
    pub decisions: Vec<RouteDecision>,
    pub standing: BTreeMap<PathBuf, String>,
    pub standing_checkouts: BTreeMap<PathBuf, PathBuf>,
    pub coverage: Value,
}

struct CoverageRun<'a> {
    settings: &'a CoverageSettings<'a>,
    lesson_vectors: BTreeMap<i64, Vec<f32>>,
    failure: Option<String>,
    projects_checked: usize,
}

async fn embed_units(
    settings: &CoverageSettings<'_>,
    units: &[Unit],
) -> Result<Vec<Vec<f32>>, String> {
    let mut vectors = Vec::with_capacity(units.len());
    for batch in units.chunks(EMBED_BATCH) {
        let texts: Vec<String> = batch.iter().map(|unit| unit.text.clone()).collect();
        let allow_remote = settings.allow_remote;
        let (_, found) = settings
            .service
            .embed_documents_checked(&texts, |config| {
                crate::semantic::egress::allow(config, allow_remote)
            })
            .await
            .map_err(|error| error.to_string())?;
        if found.len() != batch.len() {
            return Err(format!(
                "the embedding service returned {} vectors for {} texts",
                found.len(),
                batch.len()
            ));
        }
        vectors.extend(found);
    }
    Ok(vectors)
}

impl<'a> CoverageRun<'a> {
    async fn start(
        persist: &PersistHandle,
        settings: Option<&'a CoverageSettings<'a>>,
    ) -> Result<Self, String> {
        let settings = settings.ok_or_else(|| "no embedding model is enabled".to_owned())?;
        if !settings.service.is_available() {
            return Err("no embedding model is enabled".to_owned());
        }
        let model = settings.service.model_name();
        let vectors = persist
            .lessons()
            .injectable_lesson_vectors(&model)
            .await
            .map_err(|error| format!("could not read lesson embeddings: {error}"))?;
        if vectors.is_empty() {
            return Err(format!("no lesson has an embedding from model {model}"));
        }
        Ok(Self {
            settings,
            lesson_vectors: vectors.into_iter().collect(),
            failure: None,
            projects_checked: 0,
        })
    }

    async fn covered(&mut self, lessons: &[LessonRow], units: &[Unit]) -> BTreeMap<i64, String> {
        if units.is_empty() || self.failure.is_some() {
            return BTreeMap::new();
        }
        match embed_units(self.settings, units).await {
            Ok(unit_vectors) => {
                self.projects_checked += 1;
                covered_lessons(
                    lessons,
                    &Embedded {
                        units,
                        unit_vectors: &unit_vectors,
                        lesson_vectors: &self.lesson_vectors,
                    },
                    self.settings.threshold,
                )
            }
            Err(error) => {
                self.failure = Some(error);
                BTreeMap::new()
            }
        }
    }
}

fn coverage_report(run: Result<&CoverageRun<'_>, &str>, covered: usize) -> Value {
    match run {
        Ok(run) => match &run.failure {
            None => json!({
                "state": "checked",
                "projects_checked": run.projects_checked,
                "lessons_covered": covered,
                "threshold": run.settings.threshold,
            }),
            Some(error) => json!({
                "state": "skipped",
                "reason": format!("embedding failed: {error}"),
                "projects_checked": run.projects_checked,
                "lessons_covered": covered,
            }),
        },
        Err(reason) => json!({ "state": "skipped", "reason": reason }),
    }
}

pub(super) async fn route_lessons(persist: &PersistHandle, inputs: &RoutingInputs<'_>) -> Routing {
    let mut run = CoverageRun::start(persist, inputs.coverage).await;
    let mut routes = inputs.existing.clone();
    let mut decisions = Vec::new();
    let mut standing = BTreeMap::new();
    let mut standing_checkouts = BTreeMap::new();
    let mut covered_total = 0;
    for (project, lessons) in group_by_project(inputs.lessons) {
        let Some(first) = lessons.first() else {
            continue;
        };
        let roots = inputs.locations.get(&project).cloned().unwrap_or_default();
        let Some(checkout) = local_checkout(&first.project_path, &roots, inputs.eligible) else {
            continue;
        };
        let chunk_chars = inputs
            .coverage
            .map_or(1500, |settings| settings.service.chunk_max_chars());
        let covered = match run.as_mut() {
            Ok(active) => {
                let units = hand_written_units(&checkout, chunk_chars);
                active.covered(&lessons, &units).await
            }
            Err(_) => BTreeMap::new(),
        };
        covered_total += covered.len();
        let project_decisions = decide_routes(&lessons, &covered);
        let rows = standing_rows(&lessons, &project_decisions);
        if !rows.is_empty() {
            let path = checkout.join(STANDING_FILE);
            standing.insert(
                path.clone(),
                standing_body(&checkout.display().to_string(), &rows),
            );
            standing_checkouts.insert(path, checkout.clone());
        }
        for decision in &project_decisions {
            routes.insert(decision.lesson_id, decision.route);
        }
        decisions.extend(project_decisions);
    }
    let coverage = coverage_report(run.as_ref().map_err(String::as_str), covered_total);
    Routing {
        routes,
        decisions,
        standing,
        standing_checkouts,
        coverage,
    }
}
