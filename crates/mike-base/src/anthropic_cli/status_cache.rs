use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use super::cli::AnthropicCli;
use super::status::{CliStatusDto, fetch_cli_status};

const STATUS_CACHE_TTL: Duration = Duration::from_secs(5);

pub struct CliStatusCache {
    entry: RwLock<Option<(Instant, CliStatusDto)>>,
}

impl CliStatusCache {
    pub fn new() -> Self {
        Self {
            entry: RwLock::new(None),
        }
    }

    fn fresh(entry: &Option<(Instant, CliStatusDto)>) -> Option<CliStatusDto> {
        entry
            .as_ref()
            .filter(|(fetched_at, _)| fetched_at.elapsed() < STATUS_CACHE_TTL)
            .map(|(_, dto)| dto.clone())
    }

    pub async fn get_or_refresh(&self, cli: &AnthropicCli) -> CliStatusDto {
        if let Some(dto) = Self::fresh(&*self.entry.read().await) {
            return dto;
        }
        let mut guard = self.entry.write().await;
        if let Some(dto) = Self::fresh(&guard) {
            return dto;
        }
        let dto = fetch_cli_status(cli).await;
        *guard = Some((Instant::now(), dto.clone()));
        dto
    }

    pub async fn invalidate(&self) {
        *self.entry.write().await = None;
    }
}
