use serde::Serialize;

use crate::config::{CliOverrides, ProxyConfig};

use super::probes;

use super::DoctorArgs;

#[derive(Serialize)]
pub(super) struct Check {
    id: &'static str,
    required: bool,
    status: &'static str,
    code: &'static str,
    detail: String,
}

impl Check {
    pub(super) fn new(
        id: &'static str,
        required: bool,
        status: &'static str,
        code: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id,
            required,
            status,
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Serialize)]
pub(super) struct Report {
    schema_version: u8,
    product: &'static str,
    version: &'static str,
    source_sha: &'static str,
    source_dirty: bool,
    os: &'static str,
    architecture: &'static str,
    config_source: &'static str,
    read_only: bool,
    checks: Vec<Check>,
}

impl Report {
    fn new(explicit_config: bool) -> Self {
        let build = crate::change_log::current_build();
        Self {
            schema_version: 1,
            product: "mike",
            version: env!("CARGO_PKG_VERSION"),
            source_sha: build.sha,
            source_dirty: build.dirty,
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            config_source: if explicit_config {
                "selected_file"
            } else {
                "per_user_defaults_and_overrides"
            },
            read_only: true,
            checks: Vec::new(),
        }
    }

    fn failed(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.required && check.status != "pass")
    }
}

pub async fn run(args: DoctorArgs, cli: CliOverrides) -> anyhow::Result<()> {
    let mut report = Report::new(cli.config.is_some());
    match crate::config::load::load(cli) {
        Ok(config) => inspect(&config, &mut report).await,
        Err(_) => report.checks.push(Check::new(
            "configuration", true, "fail", "CONFIG_INVALID",
            "Selected configuration could not be loaded. Check its syntax and private permissions; values are not printed.",
        )),
    }
    if args.json || args.support_bundle {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("mike {} doctor (read-only)", report.version);
        for check in &report.checks {
            println!(
                "{} {} [{}]: {}",
                check.status, check.id, check.code, check.detail
            );
        }
        println!("No service, migration, client registration or trust setting was changed.");
        println!("For a safe local JSON summary to preview: mike doctor --support-bundle");
    }
    if report.failed() {
        anyhow::bail!("doctor found required failures; see the diagnostic codes above");
    }
    Ok(())
}

async fn inspect(config: &ProxyConfig, report: &mut Report) {
    report.checks.push(Check::new(
        "configuration",
        true,
        "pass",
        "CONFIG_LOADED",
        "Selected boot configuration loaded.",
    ));
    report.checks.push(if cfg!(has_spa_build) {
        Check::new(
            "embedded_ui",
            true,
            "pass",
            "UI_EMBEDDED",
            "Production UI assets are embedded in this build.",
        )
    } else {
        Check::new(
            "embedded_ui",
            true,
            "fail",
            "UI_NOT_EMBEDDED",
            "This build has no embedded production UI; it is not a complete installed release.",
        )
    });
    report.checks.extend(probes::local_tools());
    report.checks.push(probes::mcp(config).await);
    report.checks.push(probes::database(config).await);
    report.checks.push(probes::daemon(config).await);
    report.checks.push(probes::code_scan(config).await);
}
