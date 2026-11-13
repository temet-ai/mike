use serde::{Deserialize, Serialize};

use super::cli::{AnthropicCli, DEFAULT_STATUS_TIMEOUT};
use super::error::AnthropicCliError;

#[derive(Debug, Clone, Deserialize)]
struct RawAuthStatus {
    #[serde(rename = "loggedIn")]
    logged_in: bool,
    #[serde(rename = "authMethod")]
    auth_method: Option<String>,
    email: Option<String>,
    #[serde(rename = "orgId")]
    org_id: Option<String>,
    #[serde(rename = "orgName")]
    org_name: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CliStatusDto {
    pub installed: bool,
    pub binary: String,
    pub version: Option<String>,
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub email: Option<String>,
    pub org_id: Option<String>,
    pub org_name: Option<String>,
    pub subscription_type: Option<String>,
    pub error: Option<String>,
}

impl CliStatusDto {
    fn not_installed(binary: &str) -> Self {
        Self {
            installed: false,
            binary: binary.to_owned(),
            version: None,
            logged_in: false,
            auth_method: None,
            email: None,
            org_id: None,
            org_name: None,
            subscription_type: None,
            error: Some(
                "claude CLI not found on PATH — install it, or set MIKE_CLAUDE_CLI_BIN".to_owned(),
            ),
        }
    }

    fn signed_out(binary: &str, version: Option<String>, error: Option<String>) -> Self {
        Self {
            installed: true,
            binary: binary.to_owned(),
            version,
            logged_in: false,
            auth_method: None,
            email: None,
            org_id: None,
            org_name: None,
            subscription_type: None,
            error,
        }
    }

    fn from_raw(binary: &str, version: Option<String>, raw: RawAuthStatus) -> Self {
        Self {
            installed: true,
            binary: binary.to_owned(),
            version,
            logged_in: raw.logged_in,
            auth_method: raw.auth_method,
            email: raw.email,
            org_id: raw.org_id,
            org_name: raw.org_name,
            subscription_type: raw.subscription_type,
            error: None,
        }
    }
}

async fn fetch_version(cli: &AnthropicCli) -> Option<String> {
    cli.run_text(&["--version"], DEFAULT_STATUS_TIMEOUT)
        .await
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn parse_status_json(binary: &str, version: Option<String>, raw_output: &str) -> CliStatusDto {
    match serde_json::from_str::<RawAuthStatus>(raw_output.trim()) {
        Ok(raw) => CliStatusDto::from_raw(binary, version, raw),
        Err(e) => CliStatusDto::signed_out(
            binary,
            version,
            Some(format!("could not parse `claude auth status --json`: {e}")),
        ),
    }
}

pub async fn fetch_cli_status(cli: &AnthropicCli) -> CliStatusDto {
    let binary = cli.binary_label();
    let version = fetch_version(cli).await;

    match cli
        .run_text(&["auth", "status", "--json"], DEFAULT_STATUS_TIMEOUT)
        .await
    {
        Ok(stdout) => parse_status_json(&binary, version, &stdout),
        Err(AnthropicCliError::NotFound) => CliStatusDto::not_installed(&binary),
        Err(e) => CliStatusDto::signed_out(&binary, version, Some(e.to_string())),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    const FIXTURE_LOGGED_IN: &str = r#"{
        "loggedIn": true,
        "authMethod": "claude.ai",
        "apiProvider": "firstParty",
        "email": "dawid@example.com",
        "orgId": "e1dd0ba0-7c9b-4406-aa1a-a57af36ec43d",
        "orgName": "dawid@example.com's Organization",
        "subscriptionType": "max"
    }"#;

    const FIXTURE_LOGGED_OUT: &str = r#"{"loggedIn": false}"#;

    #[test]
    fn parse_status_json_logged_in_carries_account_fields() {
        let dto = parse_status_json("claude", Some("2.1.280".to_owned()), FIXTURE_LOGGED_IN);
        assert!(dto.installed);
        assert!(dto.logged_in);
        assert_eq!(dto.email.as_deref(), Some("dawid@example.com"));
        assert_eq!(dto.subscription_type.as_deref(), Some("max"));
        assert_eq!(dto.auth_method.as_deref(), Some("claude.ai"));
        assert!(dto.error.is_none());
    }

    #[test]
    fn parse_status_json_logged_out_has_no_account_fields() {
        let dto = parse_status_json("claude", Some("2.1.280".to_owned()), FIXTURE_LOGGED_OUT);
        assert!(dto.installed);
        assert!(!dto.logged_in);
        assert!(dto.email.is_none());
        assert!(dto.error.is_none());
    }

    #[test]
    fn parse_status_json_malformed_output_reports_error_not_panic() {
        let dto = parse_status_json("claude", None, "not json");
        assert!(dto.installed);
        assert!(!dto.logged_in);
        assert!(dto.error.is_some());
    }

    #[test]
    fn not_installed_reports_installed_false_with_remedy() {
        let dto = CliStatusDto::not_installed("claude");
        assert!(!dto.installed);
        assert!(!dto.logged_in);
        assert!(dto.error.as_deref().unwrap_or("").contains("PATH"));
    }
}
