use std::time::Duration;

use super::cli::AnthropicCli;
use super::error::AnthropicCliError;

const LOGOUT_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn run_logout(cli: &AnthropicCli) -> Result<(), AnthropicCliError> {
    cli.run_text(&["auth", "logout"], LOGOUT_TIMEOUT)
        .await
        .map(|_| ())
}
