use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use uuid::Uuid;

use super::drafts::DraftSets;
use super::ledger::{Ledger, Measured};
use super::model::{DraftRow, LessonRow, Recipe};
use super::owned::{Conflicts, Manifest, reconcile};
use super::plan::{PlanInputs, SkillPlan, plan_skills};
use super::settle::{SettleInputs, Settled, settle};

pub(super) const PLACEHOLDER_PATH: &str = "/work/proj";

pub(super) fn project_uuid() -> Uuid {
    Uuid::from_u128(0xabcdef12_0000_4000_8000_000000000000)
}

pub(super) fn other_uuid() -> Uuid {
    Uuid::from_u128(0x11111111_0000_4000_8000_000000000000)
}

pub(super) fn key(folder: &str) -> String {
    format!("{}:{folder}", project_uuid())
}

pub(super) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 15).unwrap_or_default()
}

pub(super) fn lesson(id: i64, topic: Option<&str>) -> LessonRow {
    LessonRow {
        id,
        text: format!("Lesson {id} text."),
        topic: topic.map(str::to_owned),
        source_turn_id: Some(900 + id),
        stage: "long_term".to_owned(),
        source: "operator".to_owned(),
        kind: "correction".to_owned(),
        project_id: 1,
        project_uuid: project_uuid(),
        project_path: PLACEHOLDER_PATH.to_owned(),
        agent_id: None,
        agent_name: None,
        placement: Some("skill".to_owned()),
        covered_by: None,
        revision: 1,
    }
}

pub(super) fn lessons(topic: &str, ids: std::ops::Range<i64>) -> Vec<LessonRow> {
    ids.map(|id| lesson(id, Some(topic))).collect()
}

pub(super) fn draft(id: i64, review: Option<&str>, watermark: i64, body: &str) -> DraftRow {
    DraftRow {
        id,
        project_uuid: project_uuid(),
        folder: "deploy-and-restart".to_owned(),
        lesson_watermark: watermark,
        revision: 1,
        review: review.map(str::to_owned),
        skill_md: format!("---\nname: deploy-and-restart\ndescription: \"D.\"\n---\n\n{body}\n"),
        reference_files: serde_json::json!([]),
        lesson_ids: Vec::new(),
    }
}

pub(super) struct Case {
    pub lessons: Vec<LessonRow>,
    pub drafts: Vec<DraftRow>,
    pub policies: BTreeMap<String, String>,
    pub user_retired: BTreeSet<String>,
    pub locations: BTreeMap<String, Vec<String>>,
    pub recipes: Vec<Recipe>,
    pub loads: BTreeMap<String, i64>,
    pub measured: Measured,
    pub cap: usize,
    pub today: NaiveDate,
}

impl Case {
    pub(super) fn new(lessons: Vec<LessonRow>) -> Self {
        Self {
            lessons,
            drafts: Vec::new(),
            policies: BTreeMap::from([(project_uuid().to_string(), "auto".to_owned())]),
            user_retired: BTreeSet::new(),
            locations: BTreeMap::new(),
            recipes: Vec::new(),
            loads: BTreeMap::new(),
            measured: Measured::new(),
            cap: 12,
            today: today(),
        }
    }
}

pub(super) struct Outcome {
    pub plan: SkillPlan,
    pub settled: Settled,
    pub conflicts: Conflicts,
}

pub(super) struct Home {
    pub dir: tempfile::TempDir,
    pub checkout: PathBuf,
    pub ledger: Ledger,
    pub manifest: Manifest,
}

impl Home {
    pub(super) fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let checkout = dir.path().join("work/proj");
        std::fs::create_dir_all(checkout.join(".git"))?;
        Ok(Self {
            dir,
            checkout,
            ledger: Ledger::new(),
            manifest: Manifest::new(),
        })
    }

    pub(super) fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    pub(super) fn skills(&self) -> PathBuf {
        self.checkout.join(".claude/skills")
    }

    pub(super) fn skill_names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.skills()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        names.sort();
        names
    }

    pub(super) fn run(&mut self, mut case: Case) -> anyhow::Result<Outcome> {
        let checkout = self.checkout.display().to_string();
        for row in &mut case.lessons {
            if row.project_path == PLACEHOLDER_PATH {
                row.project_path.clone_from(&checkout);
            }
        }
        let drafts = DraftSets::from_rows(case.drafts);
        let root = self.dir.path().to_path_buf();
        let eligible = move |path: &Path| path.starts_with(&root) && path.join(".git").exists();
        let roots = vec![self.dir.path().join("skills")];
        let runner = PathBuf::from("/repo/scripts/tool-reuse");
        let mut plan = plan_skills(
            &PlanInputs {
                today: case.today,
                lessons: &case.lessons,
                drafts: &drafts,
                policies: &case.policies,
                user_retired: &case.user_retired,
                locations: &case.locations,
                recipes: &case.recipes,
                skill_roots: &roots,
                runner: &runner,
                eligible: &eligible,
            },
            &mut self.ledger,
        );
        let settled = settle(
            &mut plan,
            &mut self.ledger,
            &SettleInputs {
                today: case.today,
                loads: &case.loads,
                cap: case.cap,
                measured: &case.measured,
            },
        );
        let state = self.state();
        let conflicts = reconcile(&plan.desired, &mut self.manifest, &state, true)?;
        Ok(Outcome {
            plan,
            settled,
            conflicts,
        })
    }
}
