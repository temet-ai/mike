use std::path::{Component, Path};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::feedback::Refused;
use super::runner::{Cmd, CmdOutput, CommandRunner, Program, StepError, git, output};

pub(crate) const PERF_DIR: &str = ".wright/perf";
pub(crate) const EVIDENCE_DIR: &str = "scripts/db-code-graders/perf-evidence";
const MEASURE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TIERS: [&str; 4] = ["hot", "read", "periodic", "migration"];
const SHOWN_QUERY_LINES: usize = 12;
const SHOWN_VALUE: usize = 300;

pub(crate) const PERF_SHAPE: &str = r#"[ { "label": string, "tier": "hot" | "read" | "periodic" | "migration", "before": ".wright/perf/<file>.sql", "after": ".wright/perf/<file>.sql", "budget_ms": number } ]"#;

pub(crate) const PERF_BRIEF: &str = "When your change adds or edits a query (a `sqlx::query` call or macro, a migration or another .sql file), the repository's commit hook demands measured evidence, and you have no database, so the executor measures it for you. \
For each changed query write a before.sql (the query as it was; leave it out for a new query) and an after.sql (the query as you changed it) under .wright/perf/ in this worktree. Each file is plain runnable SQL with real literal values taken from the finding in /wright/in/finding.json (its query text and real ids or timestamps) in place of every $1 placeholder, because the executor runs `scripts/db-perf-check` on them against the capture database. \
List each pair in the optional `perf` field of /wright/out/repair.json with a short label, the tier (hot: runs per captured request, budget 5 ms; read: per page load or API call, 50 ms; periodic: background workers and sweeps, 500 ms; migration: a one-shot backfill, 5 s), the two paths, and `budget_ms` only when the finding names a different budget for that query. \
The executor never commits the .wright/perf files. It refuses a change that touches SQL while `perf` is empty, and a measured regression or an over-budget result comes back to you as a round with db-perf-check's output.";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct PerfEntry {
    pub(crate) label: String,
    pub(crate) tier: String,
    #[serde(default)]
    pub(crate) before: Option<String>,
    pub(crate) after: String,
    #[serde(default)]
    pub(crate) budget_ms: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Perf {
    pub(crate) entries: Vec<PerfEntry>,
    pub(crate) invalid: Option<String>,
}

impl Perf {
    pub(crate) fn from_output(output: &Value) -> Self {
        match output.get("perf") {
            None | Some(Value::Null) => Self::default(),
            Some(list) => match serde_json::from_value::<Vec<PerfEntry>>(list.clone()) {
                Ok(entries) => Self {
                    entries,
                    invalid: None,
                },
                Err(e) => Self {
                    entries: Vec::new(),
                    invalid: Some(format!(
                        "the `perf` field of repair.json must be {PERF_SHAPE}: {e}; it held {}",
                        shown(&list.to_string())
                    )),
                },
            },
        }
    }
}

pub(crate) enum Evidence {
    Block(String),
    Refused(Refused),
}

fn shown(text: &str) -> String {
    text.chars().take(SHOWN_VALUE).collect()
}

fn refused(reason: String, output: Option<String>) -> Refused {
    Refused { reason, output }
}

pub(crate) fn step_error(refused: Refused) -> StepError {
    let detail = match refused.output {
        Some(text) => format!("{}\n{text}", refused.reason),
        None => refused.reason,
    };
    StepError {
        step: "perf evidence".to_owned(),
        code: None,
        detail,
    }
}

pub(crate) fn sanitize_label(raw: &str) -> String {
    let mapped: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let trimmed = mapped.trim_matches('_');
    if trimmed.is_empty() {
        "query".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn unique_label(raw: &str, taken: &[String]) -> String {
    let base = sanitize_label(raw);
    let mut label = base.clone();
    let mut n = 1;
    while taken.contains(&label) {
        n += 1;
        label = format!("{base}_{n}");
    }
    label
}

fn is_raw_call(line: &str) -> bool {
    let mut rest = line;
    while let Some(at) = rest.find("sqlx::query") {
        let tail = &rest[at + "sqlx::query".len()..];
        let tail = tail
            .strip_prefix("_as")
            .or_else(|| tail.strip_prefix("_scalar"))
            .unwrap_or(tail);
        let tail = tail.strip_prefix('!').unwrap_or(tail);
        if tail.starts_with('(') {
            return true;
        }
        rest = tail;
    }
    false
}

fn is_query_line(line: &str) -> bool {
    line.contains("query_as!")
        || line.contains("query_scalar!")
        || line.contains("query!(")
        || is_raw_call(line)
}

fn is_migration(path: &str) -> bool {
    path.ends_with(".sql") && path.split('/').any(|part| part == "migrations")
}

pub(crate) async fn sql_changes<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
) -> Result<Vec<String>, StepError> {
    let names = git(runner, worktree, &["diff", "--cached", "--name-only"]).await?;
    let mut found: Vec<String> = names
        .lines()
        .filter(|path| is_migration(path))
        .map(|path| format!("migration: {path}"))
        .collect();
    let added = git(
        runner,
        worktree,
        &[
            "diff",
            "--cached",
            "-U0",
            "--no-color",
            "--",
            "*.rs",
            ":(exclude)*/tests/*",
            ":(exclude)artifacts/*",
            ":(exclude,glob)**/perf-evidence/**",
        ],
    )
    .await?;
    found.extend(
        added
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .map(|line| line[1..].trim())
            .filter(|line| !line.starts_with("//") && is_query_line(line))
            .take(SHOWN_QUERY_LINES)
            .map(|line| format!("query line: {line}")),
    );
    Ok(found)
}

fn under_perf_dir(path: &str) -> bool {
    Path::new(path).strip_prefix(PERF_DIR).is_ok_and(|rest| {
        !rest.as_os_str().is_empty() && rest.components().all(|c| matches!(c, Component::Normal(_)))
    })
}

async fn check_entry(worktree: &Path, entry: &PerfEntry) -> Result<(), Refused> {
    if !TIERS.contains(&entry.tier.as_str()) {
        return Err(refused(
            format!(
                "perf entry `{}` names the tier `{}`; use one of hot, read, periodic or migration",
                entry.label, entry.tier
            ),
            None,
        ));
    }
    for path in std::iter::once(&entry.after).chain(entry.before.as_ref()) {
        if !under_perf_dir(path) {
            return Err(refused(
                format!(
                    "perf entry `{}` names `{path}`; its files must sit directly under {PERF_DIR}/ in this worktree",
                    entry.label
                ),
                None,
            ));
        }
        let present = tokio::fs::metadata(worktree.join(path))
            .await
            .is_ok_and(|meta| meta.is_file());
        if !present {
            return Err(refused(
                format!(
                    "perf entry `{}` names `{path}`, which does not exist in the worktree; write it with real literal values before listing it",
                    entry.label
                ),
                None,
            ));
        }
    }
    Ok(())
}

fn perf_lines(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("Perf-"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn judge(label: &str, tier: &str, out: &CmdOutput) -> Result<String, Refused> {
    let block = perf_lines(&out.stdout);
    let verdict = block
        .lines()
        .find_map(|line| line.strip_prefix("Perf-Verdict:"))
        .map(str::trim);
    match (out.code, verdict) {
        (Some(0), Some(_)) => Ok(block),
        (Some(1), Some(verdict)) => Err(refused(
            format!(
                "db-perf-check measured `{label}` ({tier} tier) as `{verdict}`: it is slower than before beyond the tier's tolerance, over the tier's budget, or has a bad plan node; change the query or the measured SQL and list it in `perf` again"
            ),
            Some(out.combined()),
        )),
        _ => Err(refused(
            format!(
                "db-perf-check could not measure `{label}` (exit {:?}); check that both .sql files run as written against the capture database with real literal values in place of every placeholder",
                out.code
            ),
            Some(out.combined()),
        )),
    }
}

async fn measure_one<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    entry: &PerfEntry,
    label: &str,
) -> Result<String, Refused> {
    check_entry(worktree, entry).await?;
    let budget = entry.budget_ms.map(|ms| ms.to_string());
    let mut args = vec!["--tier", entry.tier.as_str(), "--label", label];
    if let Some(before) = &entry.before {
        args.extend(["--before", before.as_str()]);
    }
    args.extend(["--after", entry.after.as_str()]);
    if let Some(ms) = &budget {
        args.extend(["--budget", ms.as_str()]);
    }
    let cmd = Cmd::new(Program::PerfCheck, worktree, &args).with_timeout(MEASURE_TIMEOUT);
    let out = output(runner, &cmd).await.map_err(|e| {
        refused(
            format!("db-perf-check for `{label}` could not run: {e}"),
            None,
        )
    })?;
    judge(label, &entry.tier, &out)
}

async fn discard_snapshots<R: CommandRunner>(runner: &R, worktree: &Path) {
    let cmd = Cmd::new(
        Program::Git,
        worktree,
        &["clean", "--quiet", "-fd", "--", EVIDENCE_DIR],
    );
    let _ = output(runner, &cmd).await;
}

async fn unmeasured<R: CommandRunner>(runner: &R, worktree: &Path) -> Result<Evidence, StepError> {
    let sql = sql_changes(runner, worktree).await?;
    if sql.is_empty() {
        return Ok(Evidence::Block(String::new()));
    }
    Ok(Evidence::Refused(refused(
        format!(
            "the change touches SQL but /wright/out/repair.json lists no `perf` entries; the repository's commit hook refuses SQL changes that carry no db-perf-check evidence. For each query you changed, write before.sql and after.sql with real literal values under {PERF_DIR}/ and list them in the `perf` field: {PERF_SHAPE}"
        ),
        Some(sql.join("\n")),
    )))
}

pub(crate) async fn evidence<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    perf: &Perf,
) -> Result<Evidence, StepError> {
    if let Some(reason) = &perf.invalid {
        return Ok(Evidence::Refused(refused(reason.clone(), None)));
    }
    if perf.entries.is_empty() {
        return unmeasured(runner, worktree).await;
    }
    let mut labels: Vec<String> = Vec::new();
    let mut blocks = Vec::new();
    for entry in &perf.entries {
        let label = unique_label(&entry.label, &labels);
        match measure_one(runner, worktree, entry, &label).await {
            Ok(block) => blocks.push(block),
            Err(refusal) => {
                discard_snapshots(runner, worktree).await;
                return Ok(Evidence::Refused(refusal));
            }
        }
        labels.push(label);
    }
    let stage = Cmd::new(
        Program::Git,
        worktree,
        &["add", "--all", "--", EVIDENCE_DIR],
    );
    let _ = output(runner, &stage).await;
    Ok(Evidence::Block(blocks.join("\n\n")))
}

pub(crate) fn with_block(message: &str, block: &str) -> String {
    if block.is_empty() {
        message.to_owned()
    } else {
        format!("{message}\n\n{block}")
    }
}

pub(crate) async fn clear(worktree: &Path) {
    let dir = worktree.join(PERF_DIR);
    if let Err(e) = tokio::fs::remove_dir_all(&dir).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = %e, dir = %dir.display(), "wright repair: removing the perf scratch files failed");
    }
    let _ = tokio::fs::remove_dir(worktree.join(".wright")).await;
}
