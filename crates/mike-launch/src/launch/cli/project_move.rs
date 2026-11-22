use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::CliOverrides;

#[derive(Debug, Clone, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ProjectCommand {
    Move(MoveArgs),
}

#[derive(Debug, Clone, Args)]
pub struct MoveArgs {
    pub from: String,
    pub to: PathBuf,
    #[arg(long)]
    pub machine: Option<Uuid>,
}

pub async fn run(args: ProjectArgs, overrides: CliOverrides) -> anyhow::Result<()> {
    let config = crate::config::load(overrides).context("failed to resolve config")?;
    match args.command {
        ProjectCommand::Move(request) => move_location(&config.inspect_listen_addr, request).await,
    }
}

fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    if let Ok(real) = std::fs::canonicalize(path) {
        return Ok(real);
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

pub fn request_body(request: &MoveArgs, local_machine: Option<Uuid>) -> anyhow::Result<Value> {
    let from = if Uuid::parse_str(&request.from).is_ok() {
        request.from.clone()
    } else {
        absolute(Path::new(&request.from))?
            .to_string_lossy()
            .into_owned()
    };
    let to = absolute(&request.to)?.to_string_lossy().into_owned();
    Ok(json!({"from": from, "to": to, "machine": request.machine.or(local_machine)}))
}

async fn move_location(inspect_addr: &str, request: MoveArgs) -> anyhow::Result<()> {
    let local_machine = crate::machine_identity::load_default()
        .await
        .ok()
        .map(|identity| identity.machine_uuid);
    let body = request_body(&request, local_machine)?;
    let url = format!("http://{inspect_addr}/api/projects/locations/move");
    let response = mike_base::http_client::client("project-move")
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("{url} is unreachable; is mike running?"))?;
    let status = response.status();
    let text = response.text().await?;
    anyhow::ensure!(status.is_success(), "{url} answered {status}: {text}");
    let moved: Value = serde_json::from_str(&text)?;
    println!("{}", describe(&moved));
    Ok(())
}

pub fn describe(moved: &Value) -> String {
    let count = |key: &str| moved.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    let number = |key: &str| moved.get(key).and_then(Value::as_u64).unwrap_or(0);
    let text = |key: &str| {
        moved
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned()
    };
    format!(
        "location {} moved from {} to {}: {} locations rewritten, {} merged, {} sessions repointed, {} knowledge sources rewritten",
        text("location_uuid"),
        text("from"),
        text("to"),
        count("moved"),
        count("merged"),
        number("sessions_repointed"),
        number("sources_rewritten"),
    )
}
