use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use super::observed::{
    CodexFacts, FeatureFacts, JobFacts, ModelFacts, PromptAssistantFacts, WorkerProviderFacts,
};
use crate::config::ProxyConfig;
use crate::launch::cli::locate_executable;
use crate::scheduled_jobs::{JOBS, Presence, locate};
use crate::store::persistence::PersistHandle;
use crate::tui::tcp_reachable;

const PROMPT_ASSISTANT_KEY: &str = "prompt_assistant";
const PROMPT_ASSISTANT_PORT: &str = "9985";
const TOGGLE_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn observe(config: &ProxyConfig, models: &ModelFacts) -> FeatureFacts {
    FeatureFacts {
        jobs: jobs(),
        worker_provider: worker_provider(models),
        codex: codex(models),
        prompt_assistant: prompt_assistant(toggle_enabled(config, PROMPT_ASSISTANT_KEY).await)
            .await,
    }
}

fn jobs() -> JobFacts {
    let mut facts = JobFacts {
        runnable: 0,
        missing: Vec::new(),
        needs_checkout: Vec::new(),
    };
    for job in JOBS.iter() {
        match locate(job).presence {
            Presence::Present => facts.runnable += 1,
            Presence::Missing => facts.missing.push(job.name),
            Presence::NeedsCheckout => facts.needs_checkout.push(job.name),
        }
    }
    facts
}

fn worker_provider(models: &ModelFacts) -> WorkerProviderFacts {
    let claude_on_service_path =
        crate::service::ServiceInputs::from_process()
            .ok()
            .and_then(|inputs| {
                let path = crate::service::service_path(&inputs);
                locate_executable(&crate::claude::claude_binary(), Some(OsStr::new(&path)))
            });
    WorkerProviderFacts {
        claude_on_service_path,
        openrouter_key_variable: models.openrouter_key_variable.clone(),
    }
}

fn codex(models: &ModelFacts) -> CodexFacts {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")));
    let hooks_installed = home
        .as_ref()
        .is_some_and(|home| home.join("mike").join("mike-codex-hook").is_file());
    CodexFacts {
        on_path: models.codex_on_path,
        home,
        hooks_installed,
    }
}

async fn toggle_enabled(config: &ProxyConfig, key: &str) -> bool {
    let default = crate::feature_toggles::get_toggle_enabled(
        &crate::feature_toggles::FeatureToggles::default(),
        key,
    )
    .unwrap_or(false);
    let Some(url) = config
        .database_url
        .as_deref()
        .filter(|url| config.persist_enabled && !url.trim().is_empty())
    else {
        return default;
    };
    let read = async {
        let persist = PersistHandle::connect_read_only(url).await.ok()?;
        let rows = persist.fetch_feature_toggles().await.ok();
        persist.pool().close().await;
        rows
    };
    let rows = tokio::time::timeout(TOGGLE_TIMEOUT, read)
        .await
        .ok()
        .flatten();
    rows.and_then(|rows| {
        rows.into_iter()
            .find(|(name, _, _)| name == key)
            .map(|(_, enabled, _)| enabled)
    })
    .unwrap_or(default)
}

async fn prompt_assistant(enabled: bool) -> PromptAssistantFacts {
    let port = std::env::var("PROMPT_ASSISTANT_PORT")
        .ok()
        .filter(|port| port.parse::<u16>().is_ok())
        .unwrap_or_else(|| PROMPT_ASSISTANT_PORT.to_owned());
    let addr = format!("127.0.0.1:{port}");
    let answering = enabled && tcp_reachable(&addr).await;
    PromptAssistantFacts {
        enabled,
        addr,
        answering,
    }
}
