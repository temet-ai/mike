use super::answer::{JudgeAnswer, SplitAnswer};
use super::guard::Guard;
use super::test_support::{Carrier, FileCarrier, Fixture, StubEmbedder, StubLlm, fixture};
use serde_json::json;
use uuid::Uuid;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use super::items::{Env, process_items};
use super::settings::Settings;
use super::subjects::{Run, process_subjects};
use super::tally::Tally;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

pub struct Harness {
    pub fx: Fixture,
    pub llm: StubLlm,
    pub emb: StubEmbedder,
    pub settings: Settings,
    pub guard: Guard,
    pub root: tempfile::TempDir,
}

pub async fn harness(split_models: usize) -> anyhow::Result<Option<Harness>> {
    let Some(fx) = fixture().await? else {
        return Ok(None);
    };
    Ok(Some(Harness {
        fx,
        llm: StubLlm::new(split_models),
        emb: StubEmbedder::new(),
        settings: Settings {
            settle: chrono::Duration::zero(),
            ..Settings::default()
        },
        guard: Guard::new(),
        root: tempfile::tempdir()?,
    }))
}

impl Harness {
    pub async fn items(&mut self) -> anyhow::Result<Tally> {
        let env = Env {
            persist: &self.fx.handle,
            llm: &self.llm,
            embedder: &self.emb,
            settings: &self.settings,
        };
        let mut tally = Tally::default();
        process_items(&env, &mut self.guard, &mut tally)
            .await
            .map_err(|f| anyhow::anyhow!(f.message))?;
        Ok(tally)
    }

    pub async fn subjects(&mut self, machine: Option<Uuid>) -> anyhow::Result<(Tally, i64)> {
        self.fx.make_subjects_due().await?;
        let run_id = self
            .fx
            .handle
            .lessons()
            .start_consolidation_run("knowledge", "manual")
            .await?;
        let env = Env {
            persist: &self.fx.handle,
            llm: &self.llm,
            embedder: &self.emb,
            settings: &self.settings,
        };
        let mut tally = Tally::default();
        process_subjects(&env, &mut self.guard, &mut tally, &Run { run_id, machine })
            .await
            .map_err(|f| anyhow::anyhow!(f.message))?;
        Ok((tally, run_id))
    }

    pub async fn backlog(&self) -> anyhow::Result<i64> {
        Ok(self.fx.handle.knowledge().consolidation_backlog().await?.subjects)
    }

    pub async fn statement_id(&self, carrier: &Carrier) -> anyhow::Result<i64> {
        let rows = self.fx.statements_of(carrier.item_id).await?;
        rows.iter()
            .find(|r| r.2 != "gone")
            .map(|r| r.0)
            .ok_or_else(|| anyhow::anyhow!("no statement for item {}", carrier.item_id))
    }

    pub async fn decisions(&self) -> anyhow::Result<Vec<DecisionLine>> {
        Ok(sqlx::query_as(
            "SELECT id, verdict, application, statement_id, counterpart_statement_id, text_before,
                    text_after, reason, error
               FROM memory.knowledge_consolidation_decisions ORDER BY id",
        )
        .fetch_all(self.fx.handle.pool())
        .await?)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct DecisionLine {
    pub id: i64,
    pub verdict: String,
    pub application: String,
    pub statement_id: Option<i64>,
    pub counterpart_statement_id: Option<i64>,
    pub text_before: String,
    pub text_after: Option<String>,
    pub reason: String,
    pub error: Option<String>,
}

pub fn judge(value: serde_json::Value) -> anyhow::Result<JudgeAnswer> {
    Ok(serde_json::from_value(value)?)
}

pub fn split(statements: &[(&str, &str)]) -> SplitAnswer {
    SplitAnswer {
        statements: statements
            .iter()
            .map(|(text, passage)| super::answer::RawStatement {
                text: (*text).to_owned(),
                passage: (*passage).to_owned(),
                about: String::new(),
            })
            .collect(),
    }
}

pub struct CoveredLesson {
    pub lesson: Carrier,
    pub file: FileCarrier,
    pub lesson_statement: i64,
    pub file_statement: i64,
}

pub async fn covered_lesson_scenario(h: &mut Harness, file_body: &str, passage: &str) -> anyhow::Result<CoveredLesson> {
    covered_scenario_in(h, "rules", file_body, passage).await
}

pub async fn covered_scenario_in(h: &mut Harness, source: &str, file_body: &str, passage: &str) -> anyhow::Result<CoveredLesson> {
    h.emb.set("Use jj.", &[(5, 1.0)]);
    h.emb.set("Commits use jj.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    let lesson = h.fx.lesson(project, "Use jj.").await?;
    std::fs::write(h.root.path().join("git.md"), file_body)?;
    let file = h
        .fx
        .checkout_file(project, source, "git.md", file_body, h.root.path())
        .await?;
    h.llm.push_split(0, Ok(split(&[("Commits use jj.", passage)])));
    h.items().await?;
    let lesson_statement = h.statement_id(&lesson).await?;
    let file_statement = h.statement_id(&file.carrier).await?;
    Ok(CoveredLesson {
        lesson,
        file,
        lesson_statement,
        file_statement,
    })
}

pub fn covered_answer(covered: i64, by: i64) -> anyhow::Result<JudgeAnswer> {
    judge(json!({
        "subject": "Commits use jj", "text": "Use jj for commits.",
        "members": [
            {"id": covered, "role": "covered", "by": by, "note": "says the same", "evidence_for": null},
            {"id": by, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))
}

pub const FILE_BODY: &str = "- Use jj for commits.\n- Commits use jj.\n";
pub const FILE_AFTER: &str = "- Use jj for commits.\n";

pub struct CoveredFile {
    pub file: FileCarrier,
    pub keep: i64,
    pub dup: i64,
}

pub async fn covered_file_scenario(h: &mut Harness, source: &str) -> anyhow::Result<CoveredFile> {
    h.emb.set("Use jj for commits.", &[(5, 1.0)]);
    h.emb.set("Commits use jj.", &[(5, 0.99), (6, 0.1)]);
    let project = h.fx.project().await?;
    std::fs::write(h.root.path().join("git.md"), FILE_BODY)?;
    let file = h
        .fx
        .checkout_file(project, source, "git.md", FILE_BODY, h.root.path())
        .await?;
    h.llm.push_split(
        0,
        Ok(split(&[
            ("Use jj for commits.", "Use jj for commits."),
            ("Commits use jj.", "Commits use jj."),
        ])),
    );
    h.items().await?;
    let rows = h.fx.statements_of(file.carrier.item_id).await?;
    let id_of = |text: &str| rows.iter().find(|r| r.1 == text).map(|r| r.0);
    let (Some(keep), Some(dup)) = (id_of("Use jj for commits."), id_of("Commits use jj.")) else {
        anyhow::bail!("statements missing: {rows:?}");
    };
    Ok(CoveredFile { file, keep, dup })
}
