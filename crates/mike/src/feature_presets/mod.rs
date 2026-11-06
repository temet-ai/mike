mod apply;
mod pending;
mod plan;
#[cfg(test)]
mod tests;

use clap::Args;

use crate::config::CliOverrides;
use crate::finding::Finding;
use crate::tui::client::ApiClient;

pub(crate) use apply::apply_via_api;
pub(crate) use pending::{PENDING_KEY, apply_pending_at_start, save_pending};
pub use plan::Preset;
pub(crate) use plan::{LESSONS_SETTING, default_off_keys, plan};

#[derive(Debug, Clone, Args)]
pub struct FeaturesArgs {
    #[arg(value_enum)]
    pub preset: Preset,
}

pub(crate) async fn apply_or_save(
    client: &ApiClient,
    config_file: &std::path::Path,
    preset: Preset,
) -> Vec<Finding> {
    if client.health().await {
        return apply_via_api(client, &plan(preset)).await;
    }
    match save_pending(config_file, preset) {
        Ok(()) => vec![Finding::note(format!(
            "The proxy is not running, so the choice is saved as {PENDING_KEY} = {} in {} and is applied when the proxy next starts.",
            preset.as_str(),
            config_file.display()
        ))],
        Err(e) => vec![Finding::failed(format!(
            "The proxy is not running, and the choice could not be saved: {e}"
        ))],
    }
}

pub async fn run(args: FeaturesArgs, overrides: CliOverrides) -> i32 {
    let config = match crate::config::load(overrides) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("mike features: the configuration could not be resolved: {e:#}");
            return 1;
        }
    };
    let file = match crate::user_config::resolved_config_file() {
        Ok(file) => file,
        Err(e) => {
            eprintln!("mike features: {e}");
            return 1;
        }
    };
    let client = ApiClient::new(&config);
    let mut code = 0;
    for finding in apply_or_save(&client, &file, args.preset).await {
        if finding.passed() {
            println!("{}", finding.text);
        } else {
            eprintln!("{}", finding.text);
            code = 1;
        }
    }
    code
}
