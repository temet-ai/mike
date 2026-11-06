mod installation;
mod probes;

mod check_background;
mod check_claude;
mod checks;
mod observe;
mod observe_db;
mod observed;
mod render;

#[cfg(test)]
mod tests;

use clap::Args;
use serde::Serialize;

use crate::config::{CliOverrides, ProxyConfig};

pub(crate) use observe::{InspectView, connect_addr};

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub support_bundle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SetupStep {
    Database,
    Service,
    Claude,
    Models,
    Embeddings,
    Terminal,
    Features,
}

impl SetupStep {
    pub(crate) fn name(self) -> &'static str {
        match self {
            SetupStep::Database => "database",
            SetupStep::Service => "service",
            SetupStep::Claude => "claude",
            SetupStep::Models => "models",
            SetupStep::Embeddings => "embeddings",
            SetupStep::Terminal => "terminal",
            SetupStep::Features => "features",
        }
    }

    pub(crate) fn command(self) -> String {
        format!("mike setup {}", self.name())
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Check {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) status: Status,
    pub(crate) detail: String,
    pub(crate) setup_step: Option<SetupStep>,
    pub(crate) fix: Option<String>,
}

impl Check {
    pub(crate) fn ok(id: &'static str, name: &'static str, detail: impl Into<String>) -> Self {
        Self::new(id, name, Status::Ok, detail, None)
    }

    pub(crate) fn warn(
        id: &'static str,
        name: &'static str,
        detail: impl Into<String>,
        step: Option<SetupStep>,
    ) -> Self {
        Self::new(id, name, Status::Warn, detail, step)
    }

    pub(crate) fn fail(
        id: &'static str,
        name: &'static str,
        detail: impl Into<String>,
        step: Option<SetupStep>,
    ) -> Self {
        Self::new(id, name, Status::Fail, detail, step)
    }

    fn new(
        id: &'static str,
        name: &'static str,
        status: Status,
        detail: impl Into<String>,
        step: Option<SetupStep>,
    ) -> Self {
        Self {
            id,
            name,
            status,
            detail: detail.into(),
            setup_step: step,
            fix: step.map(SetupStep::command),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Report {
    pub(crate) version: String,
    pub(crate) passed: bool,
    pub(crate) ok: usize,
    pub(crate) warn: usize,
    pub(crate) fail: usize,
    pub(crate) checks: Vec<Check>,
}

impl Report {
    pub(crate) fn new(checks: Vec<Check>) -> Self {
        let count = |status| checks.iter().filter(|c| c.status == status).count();
        Self {
            version: crate::cli::version_line(),
            passed: count(Status::Fail) == 0,
            ok: count(Status::Ok),
            warn: count(Status::Warn),
            fail: count(Status::Fail),
            checks,
        }
    }

    pub(crate) fn exit_code(&self) -> i32 {
        i32::from(!self.passed)
    }
}

pub async fn run(args: DoctorArgs, overrides: CliOverrides) -> anyhow::Result<()> {
    if args.json || args.support_bundle {
        return installation::run(args, overrides).await;
    }
    let report = match crate::config::load(overrides) {
        Ok(config) => build_report(&config).await,
        Err(e) => Report::new(vec![Check::fail(
            "config",
            "configuration",
            format!("the configuration could not be resolved: {e:#}"),
            Some(SetupStep::Database),
        )]),
    };
    print!("{}", render::text(&report));
    anyhow::ensure!(report.passed, "doctor found required failures");
    Ok(())
}

pub(crate) async fn database_report(url: &str) -> Report {
    let facts = observed::DatabaseFacts {
        persist_enabled: true,
        redacted_url: Some(crate::config::load::redact_database_url(url)),
        server: Some(observe_db::inspect_server(url).await),
    };
    Report::new(checks::database(&facts))
}

pub(crate) fn render_report(title: &str, report: &Report) -> String {
    render::titled(title, report)
}

pub(crate) async fn build_report(config: &ProxyConfig) -> Report {
    let observed = observe::observe(config).await;
    Report::new(checks::evaluate(&observed))
}
