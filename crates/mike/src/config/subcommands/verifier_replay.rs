use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::VerifierReplayArgs;
use crate::config::{CliOverrides, ProxyConfig, load};
use crate::llm::LlmRole;
use crate::llm::usage::LlmUsageSink;
use crate::settings::{SharedSettingsOverlay, apply_db_rows_to_overlay, steerer_live_source};
use crate::steerer::fleet::{
    FileSink, ItemReport, RecordedCall, ReplayItem, ReplayMode, ReplaySteerer, VerifierInputBudget,
    load_replay_steerer, parity_recorded, replay_client, replay_items,
};
use crate::store::persistence::PersistHandle;

const PROMPT_USD_PER_MILLION_TOKENS: f64 = 0.233;
const COMPLETION_USD_PER_MILLION_TOKENS: f64 = 1.166;
const TYPICAL_COMPLETION_TOKENS: f64 = 841.0;

pub(super) async fn verifier_replay(args: VerifierReplayArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for verifier-replay")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "verifier-replay requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let ph = PersistHandle::connect_read_only(database_url)
        .await
        .context("verifier-replay: could not connect to the capture database")?;
    let mut replay = load_replay_steerer(&ph)
        .await
        .context("verifier-replay: could not load the verifier steerer")?;
    if let Some(path) = &args.prompt_file {
        replay.system = std::fs::read_to_string(path)
            .with_context(|| format!("verifier-replay: could not read {}", path.display()))?;
        println!(
            "verifier-replay: judging with the prompt in {}",
            path.display()
        );
    }
    match (&args.parity_recorded, &args.out) {
        (Some(recorded), Some(out)) => {
            run_parity(&ph, &replay, recorded, out, args.dump_parity.as_deref()).await
        }
        _ => run_replay(&ph, &replay, &config, &args).await,
    }
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).with_context(|| {
                format!("{}:{}: not a valid record: {line}", path.display(), i + 1)
            })
        })
        .collect()
}

fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) -> Result<()> {
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    Ok(())
}

async fn run_parity(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    recorded: &Path,
    out: &Path,
    dump_dir: Option<&Path>,
) -> Result<()> {
    let calls: Vec<RecordedCall> = read_jsonl(recorded)?;
    let rows = parity_recorded(ph, replay, &calls, dump_dir).await;
    let mut summary: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        *summary.entry(tag_of(&row.result)).or_default() += 1;
    }
    let report = serde_json::json!({ "summary": summary, "rows": rows });
    std::fs::write(out, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("could not write {}", out.display()))?;
    println!("parity over {} recorded calls: {summary:?}", calls.len());
    Ok(())
}

fn tag_of<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| {
            v.get("status")
                .or_else(|| v.get("kind"))
                .and_then(|t| t.as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

async fn run_replay(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    config: &ProxyConfig,
    args: &VerifierReplayArgs,
) -> Result<()> {
    let set = args
        .set
        .as_deref()
        .context("verifier-replay needs --set or --parity-recorded")?;
    let items: Vec<ReplayItem> = read_jsonl(set)?;
    let budget = VerifierInputBudget::from_setting(&args.arm);
    let reports = if args.execute {
        execute(ph, replay, config, args, &items, budget).await?
    } else {
        replay_items(ph, replay, &items, budget, &ReplayMode::DryRun).await
    };
    if let Some(dump) = &args.dump {
        write_dump(dump, &reports)?;
    }
    if let Some(out) = &args.out {
        write_jsonl(out, &reports)?;
    }
    print_run_report(&args.arm, args.execute, &reports);
    Ok(())
}

async fn execute(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    config: &ProxyConfig,
    args: &VerifierReplayArgs,
    items: &[ReplayItem],
    budget: VerifierInputBudget,
) -> Result<Vec<ItemReport>> {
    let out = args.out.as_deref().context("--execute requires --out")?;
    let calls_path = calls_path_for(out);
    let sink = Arc::new(
        FileSink::create(&calls_path)
            .with_context(|| format!("could not create {}", calls_path.display()))?,
    );
    let client = build_client(ph, replay, config, args, &sink).await?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: args.max_usd,
    };
    let reports = replay_items(ph, replay, items, budget, &mode).await;
    println!(
        "recorded {} calls, {:.4} USD, in {}",
        sink.calls(),
        sink.total_usd(),
        calls_path.display()
    );
    Ok(reports)
}

fn calls_path_for(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_owned();
    name.push(".calls.jsonl");
    PathBuf::from(name)
}

const CONFIG_KEY_SOURCE: &str = "the loaded config (steerer.openrouter_api_key)";
const KEY_ENV_VARS: [&str; 2] = ["MIKE_STEERER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"];

fn resolve_api_key(
    configured: Option<String>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<(String, &'static str)> {
    let candidates = std::iter::once((CONFIG_KEY_SOURCE, configured))
        .chain(KEY_ENV_VARS.iter().map(|name| (*name, env(name))));
    for (source, value) in candidates {
        match value {
            Some(v) if v.trim().is_empty() => {
                anyhow::bail!("--execute: the OpenRouter key from {source} is empty")
            }
            Some(v) => return Ok((v, source)),
            None => {}
        }
    }
    anyhow::bail!("--execute needs MIKE_STEERER_OPENROUTER_API_KEY")
}

async fn build_client(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    config: &ProxyConfig,
    args: &VerifierReplayArgs,
    sink: &Arc<FileSink>,
) -> Result<crate::llm::OpenRouterClient> {
    let steerer_config = &config.steerer;
    let configured = steerer_config
        .openrouter_api_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned());
    let model = args
        .model
        .clone()
        .or_else(|| (!replay.steerer.model.is_empty()).then(|| replay.steerer.model.clone()))
        .unwrap_or_else(|| steerer_config.model.clone());
    let mut overlay = apply_db_rows_to_overlay(ph.fetch_settings().await?);
    let needs_key =
        crate::llm::key::model_needs_openrouter_key(&overlay.llm_provider, None, &model);
    let (key, source) = match resolve_api_key(configured, |name| std::env::var(name).ok()) {
        Ok(found) => found,
        Err(_) if !needs_key => (
            String::new(),
            "none (the replay model needs no OpenRouter key)",
        ),
        Err(e) => {
            return Err(e.context(format!("the replay model `{model}` runs on OpenRouter")));
        }
    };
    println!("verifier-replay: OpenRouter key taken from {source}");
    if let Some(effort) = &args.reasoning_effort {
        overlay.steerer_reasoning_effort = effort.clone();
    }
    let shared: SharedSettingsOverlay = Arc::new(ArcSwap::from_pointee(overlay));
    let dyn_sink: Arc<dyn LlmUsageSink> = sink.clone();
    Ok(replay_client(
        key,
        model,
        Some(steerer_config.openrouter_base_url.clone()),
        Some(Duration::from_secs(steerer_config.request_timeout_secs)),
    )?
    .with_attribution(LlmRole::Steerer, Some(&dyn_sink))
    .with_live_settings(steerer_live_source(&shared)))
}

fn write_dump(path: &Path, reports: &[ItemReport]) -> Result<()> {
    let records: Vec<serde_json::Value> = reports
        .iter()
        .map(|r| {
            serde_json::json!({
                "turn_id": r.turn_id,
                "trigger_request_id": r.trigger_request_id,
                "gate_admitted": r.gate_admitted,
                "sizes": r.sizes,
                "known_differences": r.known_differences,
                "user_message": r.user_message,
            })
        })
        .collect();
    write_jsonl(path, &records)
}

fn print_run_report(arm: &str, executed: bool, reports: &[ItemReport]) {
    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    for report in reports {
        *outcomes.entry(tag_of(&report.outcome)).or_default() += 1;
    }
    let admitted: Vec<&ItemReport> = reports.iter().filter(|r| r.gate_admitted).collect();
    let prompt_tokens: u64 = admitted
        .iter()
        .map(|r| r.sizes.projected_prompt_tokens())
        .sum();
    let projected_usd = prompt_tokens as f64 * PROMPT_USD_PER_MILLION_TOKENS / 1e6
        + admitted.len() as f64 * TYPICAL_COMPLETION_TOKENS * COMPLETION_USD_PER_MILLION_TOKENS
            / 1e6;
    let mode = if executed {
        "executed"
    } else {
        "dry run, nothing sent"
    };
    println!(
        "verifier-replay arm {arm} ({mode}): {} items, {} pass the pattern gate, {outcomes:?}",
        reports.len(),
        admitted.len()
    );
    println!(
        "one verdict call per admitted item: about {prompt_tokens} prompt tokens, \
         about {projected_usd:.4} USD before quorum calls"
    );
}

#[cfg(test)]
mod key_tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    fn error_text(result: Result<(String, &'static str)>) -> String {
        result.err().map(|e| e.to_string()).unwrap_or_default()
    }

    #[test]
    fn empty_key_is_rejected_naming_the_source_not_the_value() {
        let err = error_text(resolve_api_key(
            None,
            env_of(&[("MIKE_STEERER_OPENROUTER_API_KEY", "  ")]),
        ));
        assert!(err.contains("MIKE_STEERER_OPENROUTER_API_KEY"), "{err}");
        let err = error_text(resolve_api_key(Some(String::new()), env_of(&[])));
        assert!(err.contains("loaded config"), "{err}");
    }

    #[test]
    fn first_present_key_wins_and_names_its_source() -> Result<()> {
        let (key, source) = resolve_api_key(
            None,
            env_of(&[
                ("OPENROUTER_API_KEY", "sk-b"),
                ("MIKE_STEERER_OPENROUTER_API_KEY", "sk-a"),
            ]),
        )?;
        assert_eq!(
            (key.as_str(), source),
            ("sk-a", "MIKE_STEERER_OPENROUTER_API_KEY")
        );
        let (key, source) = resolve_api_key(None, env_of(&[("OPENROUTER_API_KEY", "sk-b")]))?;
        assert_eq!((key.as_str(), source), ("sk-b", "OPENROUTER_API_KEY"));
        let (key, source) = resolve_api_key(Some("sk-c".to_owned()), env_of(&[]))?;
        assert_eq!((key.as_str(), source), ("sk-c", CONFIG_KEY_SOURCE));
        Ok(())
    }

    #[test]
    fn no_key_anywhere_is_an_error() {
        assert!(resolve_api_key(None, env_of(&[])).is_err());
    }
}
