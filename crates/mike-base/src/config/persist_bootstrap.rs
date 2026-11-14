use tokio_util::task::TaskTracker;

use crate::llm::{DailySpendLedger, LlmCallRecord, LlmRole, LlmUsageSink, RoleSpend};
use crate::store::persistence::llm_calls::LlmCallInsert;
use crate::store::persistence::pool_classes::PoolClassesConfig;
use crate::store::persistence::{PersistError, PersistHandle, SchemaPolicy};

use super::load;
use super::schema::ProxyConfig;

pub struct PersistLlmUsageSink {
    persist: PersistHandle,
    tracker: TaskTracker,
    spent_today: DailySpendLedger,
}

async fn seed_todays_spend(persist: &PersistHandle) -> RoleSpend {
    let day_start = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|naive| naive.and_utc());
    match persist.llm_spend_by_role(day_start, None).await {
        Ok(rows) => {
            let mut seed = RoleSpend::default();
            for row in rows {
                if let Some(role) = LlmRole::from_db_value(&row.key) {
                    seed.add(role, row.cost_usd);
                }
            }
            seed
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "llm usage sink: failed to seed today's per-role spend from the database — starting at $0"
            );
            RoleSpend::default()
        }
    }
}

impl PersistLlmUsageSink {
    pub async fn new(persist: PersistHandle, tracker: TaskTracker) -> Self {
        let seed = seed_todays_spend(&persist).await;
        Self {
            persist,
            tracker,
            spent_today: DailySpendLedger::seeded(seed),
        }
    }
}

impl LlmUsageSink for PersistLlmUsageSink {
    fn record(&self, call: LlmCallRecord) {
        if let Some(cost) = call.cost_usd {
            self.spent_today.add(call.role, cost);
        }

        let Some(role) = call.role.as_db_value() else {
            tracing::warn!(
                model = %call.model,
                outcome = call.outcome.as_str(),
                cost_usd = ?call.cost_usd,
                "llm usage sink: call from a client with no role — not persisted; tag the client with `with_role`"
            );
            return;
        };

        let persist = self.persist.clone();
        self.tracker.spawn(async move {
            let insert = LlmCallInsert {
                role,
                model: &call.model,
                generation_id: call.generation_id.as_deref(),
                prompt_tokens: call.prompt_tokens,
                completion_tokens: call.completion_tokens,
                reasoning_tokens: call.reasoning_tokens,
                cached_tokens: call.cached_tokens,
                cost_usd: call.cost_usd,
                latency_ms: call.latency_ms,
                outcome: call.outcome.as_str(),
                http_status: call.http_status,
                session_id: call.session_id,
                steerer_id: call.steerer_id,
                operator_turn_id: call.operator_turn_id,
                request_body: call.request_body,
                response_body: call.response_body,
            };
            if let Err(e) = persist.insert_llm_call(insert).await {
                tracing::warn!(
                    error = %e,
                    "llm usage sink: failed to record call — spend/usage tracking for this call is lost"
                );
            }
        });
    }

    fn spent_today(&self) -> RoleSpend {
        self.spent_today.snapshot()
    }
}

pub async fn build_persist_handle(
    config: &ProxyConfig,
    policy: SchemaPolicy,
) -> anyhow::Result<Option<PersistHandle>> {
    if !config.persist_enabled {
        return Ok(None);
    }

    let database_url = config
        .database_url
        .as_deref()
        .ok_or(PersistError::MissingDatabaseUrl)?;

    let fallback_eligible = config.db_fallback_enabled && config.local_database_url.is_some();
    let connect = Connect {
        connect_timeout: std::time::Duration::from_millis(config.db_connect_timeout_ms),
        pools: config.db_pool.classes(),
        policy,
    };

    if fallback_eligible && primary_tcp_unreachable(database_url).await {
        let handle = fall_back_to_local(
            config,
            connect,
            "remote DB unreachable (fast probe), falling back to local capture DB",
        )
        .await?;
        return Ok(Some(handle));
    }

    Ok(Some(
        connect_with_primary_then_fallback(config, database_url, fallback_eligible, connect)
            .await?,
    ))
}

async fn connect_with_primary_then_fallback(
    config: &ProxyConfig,
    database_url: &str,
    fallback_eligible: bool,
    connect: Connect,
) -> Result<PersistHandle, PersistError> {
    match connect_and_preflight(database_url, connect).await {
        Ok(handle) => Ok(handle),
        Err(PersistError::Connect { .. }) if fallback_eligible => {
            fall_back_to_local(
                config,
                connect,
                "remote DB unreachable, falling back to local capture DB",
            )
            .await
        }
        Err(e) => Err(e),
    }
}

async fn fall_back_to_local(
    config: &ProxyConfig,
    connect: Connect,
    reason: &str,
) -> Result<PersistHandle, PersistError> {
    let local_url = config
        .local_database_url
        .as_deref()
        .ok_or(PersistError::MissingDatabaseUrl)?;
    tracing::warn!(
        local_database_url = %load::redact_database_url(local_url),
        "{reason}"
    );
    let handle = connect_and_preflight(local_url, connect).await?;
    Ok(handle.into_local_fallback())
}

pub async fn primary_tcp_unreachable(database_url: &str) -> bool {
    const PRIMARY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

    let Some((host, port)) = parse_host_port(database_url) else {
        return false;
    };
    match tokio::time::timeout(
        PRIMARY_PROBE_TIMEOUT,
        tokio::net::TcpStream::connect((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(_stream)) => false,
        Ok(Err(_)) | Err(_) => true,
    }
}

pub fn parse_host_port(url: &str) -> Option<(String, u16)> {
    let (_scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    if host_port.is_empty() {
        return None;
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Some((host.to_string(), port.parse().ok()?)),
        Some(_) => None,
        None => Some((host_port.to_string(), 5432)),
    }
}

#[derive(Debug, Clone, Copy)]
struct Connect {
    connect_timeout: std::time::Duration,
    pools: PoolClassesConfig,
    policy: SchemaPolicy,
}

async fn connect_and_preflight(
    database_url: &str,
    connect: Connect,
) -> Result<PersistHandle, PersistError> {
    let handle = PersistHandle::connect_classed(
        database_url,
        connect.connect_timeout,
        &connect.pools,
        connect.policy,
    )
    .await?;
    handle.preflight().await?;
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn build_persist_handle_no_local_url_propagates_connect() -> anyhow::Result<()> {
        let config = ProxyConfig {
            persist_enabled: true,
            database_url: Some("postgres://nobody:nobody@127.0.0.1:1/does_not_exist".to_string()),
            db_fallback_enabled: true,
            local_database_url: None,
            db_connect_timeout_ms: 200,
            ..ProxyConfig::default()
        };
        match build_persist_handle(&config, SchemaPolicy::Migrate).await {
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("failed to connect"),
                    "expected a connect error, got: {msg}"
                );
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected connect error with no fallback URL"),
        }
    }

    #[tokio::test]
    async fn build_persist_handle_fallback_disabled_propagates_connect() -> anyhow::Result<()> {
        let config = ProxyConfig {
            persist_enabled: true,
            database_url: Some("postgres://nobody:nobody@127.0.0.1:1/primary_down".to_string()),
            db_fallback_enabled: false,
            local_database_url: Some(
                "postgres://nobody:nobody@127.0.0.1:2/local_fallback".to_string(),
            ),
            db_connect_timeout_ms: 200,
            ..ProxyConfig::default()
        };
        match build_persist_handle(&config, SchemaPolicy::Migrate).await {
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("primary_down"),
                    "the PRIMARY connect error must propagate when fallback is disabled: {msg}"
                );
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected primary connect error, fallback disabled"),
        }
    }

    #[tokio::test]
    async fn build_persist_handle_attempts_local_when_primary_unreachable() -> anyhow::Result<()> {
        let config = ProxyConfig {
            persist_enabled: true,
            database_url: Some("postgres://nobody:nobody@127.0.0.1:1/primary_down".to_string()),
            db_fallback_enabled: true,
            local_database_url: Some(
                "postgres://nobody:nobody@127.0.0.1:2/local_fallback".to_string(),
            ),
            db_connect_timeout_ms: 200,
            ..ProxyConfig::default()
        };
        match build_persist_handle(&config, SchemaPolicy::Migrate).await {
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    msg.contains("local_fallback"),
                    "fallback must be attempted against the LOCAL url: {msg}"
                );
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected local connect error (both URLs unreachable)"),
        }
    }

    #[tokio::test]
    async fn build_persist_handle_disabled_returns_none() -> anyhow::Result<()> {
        let config = ProxyConfig {
            persist_enabled: false,
            database_url: Some("postgres://nobody:nobody@127.0.0.1:1/does_not_exist".to_string()),
            db_fallback_enabled: true,
            local_database_url: Some("postgres://nobody:nobody@127.0.0.1:2/local".to_string()),
            ..ProxyConfig::default()
        };
        assert!(
            build_persist_handle(&config, SchemaPolicy::Migrate)
                .await?
                .is_none(),
            "persistence disabled must short-circuit to Ok(None)"
        );
        Ok(())
    }

    #[test]
    fn parse_host_port_handles_url_shapes() {
        assert_eq!(
            parse_host_port("postgres://user:pass@db.internal:6543/claude_proxy"),
            Some(("db.internal".to_string(), 6543))
        );
        assert_eq!(
            parse_host_port("postgres://user@127.0.0.1:6666/claude_proxy?sslmode=require"),
            Some(("127.0.0.1".to_string(), 6666))
        );
        assert_eq!(
            parse_host_port("postgres://host.example/db"),
            Some(("host.example".to_string(), 5432))
        );
        assert_eq!(parse_host_port("postgres:///var/run/pg/db"), None);
        assert_eq!(parse_host_port("not a url"), None);
    }

    #[tokio::test]
    async fn primary_tcp_unreachable_true_for_refused_port() {
        assert!(primary_tcp_unreachable("postgres://u:p@127.0.0.1:1/db").await);
    }

    #[tokio::test]
    async fn primary_tcp_unreachable_false_for_unparseable_url() {
        assert!(!primary_tcp_unreachable("postgres:///socket/only").await);
    }
}
