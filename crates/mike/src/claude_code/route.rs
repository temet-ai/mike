use std::path::Path;

use clap::ValueEnum;

use super::error::ClaudeCodeError;
use super::settings::{self, ROUTE_KEY};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Route {
    Sidecar,
    Plain,
}

impl Route {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Route::Sidecar => "sidecar",
            Route::Plain => "plain",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "sidecar" => Some(Route::Sidecar),
            "plain" => Some(Route::Plain),
            _ => None,
        }
    }

    pub(crate) fn describe(self) -> &'static str {
        match self {
            Route::Sidecar => {
                "through the forwarding sidecar, which also captures Remote Control traffic (`mike claude`)"
            }
            Route::Plain => {
                "straight to the proxy with ANTHROPIC_BASE_URL, which needs no certificate and does not capture Remote Control traffic"
            }
        }
    }
}

pub(crate) fn stored(config_file: &Path) -> Result<Option<Route>, ClaudeCodeError> {
    Ok(settings::read(config_file, ROUTE_KEY)?.and_then(|text| Route::parse(&text)))
}

pub(crate) fn store(config_file: &Path, route: Route) -> Result<(), ClaudeCodeError> {
    settings::write_text(config_file, ROUTE_KEY, route.as_str())
}
