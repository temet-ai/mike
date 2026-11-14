use super::observed::{
    ConfigFileFacts, DatabaseFacts, EmbeddingFacts, MigrationFacts, ModelFacts, Observed,
    RerankFacts, ServerFacts, ServiceFacts, SidecarFacts, SpaFacts, ToolFacts,
};
use super::{Check, SetupStep};
use crate::user_config::FileState;

pub(crate) fn evaluate(observed: &Observed) -> Vec<Check> {
    let mut checks = vec![config_file(&observed.config_file), version(observed)];
    checks.extend(database(&observed.database));
    checks.extend(
        observed
            .services
            .iter()
            .map(|s| service(s, &observed.build.sha)),
    );
    checks.push(spa(observed.spa));
    checks.extend(sidecar(&observed.sidecar));
    checks.extend(super::check_background::background_service(
        observed.background.as_ref(),
    ));
    checks.push(claude(&observed.claude));
    checks.extend(super::check_claude::mcp(&observed.mcp));
    checks.extend(super::check_claude::connection(&observed.connection));
    checks.extend(models(&observed.models));
    checks.push(embeddings(&observed.embeddings));
    checks.push(rerank(&observed.rerank));
    checks.push(launch_backend(&observed.launch_backend));
    checks.push(super::code_scan::check(observed.code_scan.as_ref()));
    checks
}

fn config_file(facts: &ConfigFileFacts) -> Check {
    let shown = facts.path.as_ref().map_or_else(
        || "unknown location".to_string(),
        |p| p.display().to_string(),
    );
    match &facts.state {
        FileState::Loaded { keys } => Check::ok(
            "config_file",
            "config file",
            format!("{shown} is readable and holds {keys} settings"),
        ),
        FileState::Missing => Check::warn(
            "config_file",
            "config file",
            format!("{shown} does not exist yet, so mike runs on defaults and the environment"),
            Some(SetupStep::Database),
        ),
        FileState::NoLocation(why) => Check::fail(
            "config_file",
            "config file",
            format!("the location of the config file is unknown: {why}"),
            None,
        ),
        FileState::Unusable(why) => Check::fail(
            "config_file",
            "config file",
            format!("{shown} cannot be used: {why}"),
            None,
        ),
    }
}

fn version(observed: &Observed) -> Check {
    Check::ok("version", "mike version", observed.build.line.clone())
}

pub(super) fn database(facts: &DatabaseFacts) -> Vec<Check> {
    let mut checks = vec![database_url(facts)];
    let Some(server) = &facts.server else {
        return checks;
    };
    match server {
        Err(why) => checks.push(Check::fail(
            "database_reachable",
            "database connection",
            format!("the database did not answer: {why}"),
            Some(SetupStep::Database),
        )),
        Ok(server) => {
            checks.push(Check::ok(
                "database_reachable",
                "database connection",
                "connected with a read-only session",
            ));
            checks.push(postgres_version(server));
            checks.push(extensions(server));
            checks.push(migrations(&server.migrations));
        }
    }
    checks
}

fn database_url(facts: &DatabaseFacts) -> Check {
    match (&facts.redacted_url, facts.persist_enabled) {
        (Some(url), _) => Check::ok("database_url", "database url", url.clone()),
        (None, false) => Check::warn(
            "database_url",
            "database url",
            "capture history is switched off, so nothing is stored between runs",
            Some(SetupStep::Database),
        ),
        (None, true) => Check::fail(
            "database_url",
            "database url",
            "persistence is on but no database URL is set (database_url in the config file or MIKE_DATABASE_URL)",
            Some(SetupStep::Database),
        ),
    }
}

fn postgres_version(server: &ServerFacts) -> Check {
    let (major, minor) = (server.version_num / 10_000, server.version_num % 10_000);
    let shown = format!("{major}.{minor}");
    if major == crate::store::persistence::pool::SERVER_MAJOR {
        Check::ok(
            "postgres_version",
            "postgres version",
            format!("PostgreSQL {shown}"),
        )
    } else {
        Check::fail(
            "postgres_version",
            "postgres version",
            format!("PostgreSQL {shown} is unsupported; mike requires PostgreSQL 19"),
            Some(SetupStep::Database),
        )
    }
}

fn extensions(server: &ServerFacts) -> Check {
    let missing: Vec<_> = server.extensions.iter().filter(|e| !e.installed).collect();
    if missing.is_empty() {
        return Check::ok(
            "database_extensions",
            "database extensions",
            "vector, pgcrypto and pg_trgm are installed",
        );
    }
    let unavailable: Vec<&str> = missing
        .iter()
        .filter(|e| !e.available)
        .map(|e| e.name)
        .collect();
    if !unavailable.is_empty() {
        return Check::fail(
            "database_extensions",
            "database extensions",
            format!(
                "the server does not provide {}; install the matching Postgres package (pgvector for vector)",
                unavailable.join(", ")
            ),
            Some(SetupStep::Database),
        );
    }
    let names: Vec<&str> = missing.iter().map(|e| e.name).collect();
    let forbidden: Vec<&str> = missing
        .iter()
        .filter(|e| !e.creatable)
        .map(|e| e.name)
        .collect();
    if !forbidden.is_empty() {
        return Check::fail(
            "database_extensions",
            "database extensions",
            format!(
                "{} not created yet, and this database user may not create {}; connect once as a superuser and run CREATE EXTENSION for each",
                names.join(", "),
                forbidden.join(", ")
            ),
            Some(SetupStep::Database),
        );
    }
    Check::ok(
        "database_extensions",
        "database extensions",
        format!(
            "{} can be created; the first start does it",
            names.join(", ")
        ),
    )
}

fn migrations(facts: &Result<MigrationFacts, String>) -> Check {
    match facts {
        Err(why) => Check::fail(
            "database_migrations",
            "database migrations",
            format!("the migration state could not be read: {why}"),
            Some(SetupStep::Database),
        ),
        Ok(m) if !m.blocking.is_empty() => Check::fail(
            "database_migrations",
            "database migrations",
            format!(
                "this build and the database disagree: {}",
                m.blocking.join("; ")
            ),
            Some(SetupStep::Service),
        ),
        Ok(m) if m.pending > 0 => Check::warn(
            "database_migrations",
            "database migrations",
            format!(
                "{} migrations are waiting; `mike serve` applies them on start",
                m.pending
            ),
            Some(SetupStep::Service),
        ),
        Ok(_) => Check::ok("database_migrations", "database migrations", "up to date"),
    }
}

fn service_ids(name: &str) -> (&'static str, &'static str) {
    match name {
        "proxy" => ("proxy_port", "proxy port"),
        "inspect" => ("inspect_port", "inspect port"),
        _ => ("spa_port", "web UI port"),
    }
}

fn service(facts: &ServiceFacts, this_sha: &str) -> Check {
    let (id, name) = service_ids(facts.name);
    if !facts.enabled {
        return Check::ok(id, name, format!("{} is switched off", facts.name));
    }
    if !facts.answering {
        let fail = facts.name != "spa";
        let detail = format!("nothing answers on {}", facts.addr);
        return if fail {
            Check::fail(id, name, detail, Some(SetupStep::Service))
        } else {
            Check::warn(id, name, detail, Some(SetupStep::Service))
        };
    }
    match &facts.running_sha {
        Some(running) if running != this_sha => Check::warn(
            id,
            name,
            format!(
                "{} answers, but it runs build {} and this binary is build {}; restart it",
                facts.addr,
                short(running),
                short(this_sha)
            ),
            Some(SetupStep::Service),
        ),
        Some(_) => Check::ok(
            id,
            name,
            format!("{} answers and runs this build", facts.addr),
        ),
        None => Check::ok(id, name, format!("{} answers", facts.addr)),
    }
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

fn spa(facts: SpaFacts) -> Check {
    match (facts.embedded, facts.enabled) {
        (true, _) => Check::ok(
            "spa_built",
            "web UI in the binary",
            "built into this binary",
        ),
        (false, false) => Check::ok(
            "spa_built",
            "web UI in the binary",
            "not built in, and the web UI is switched off",
        ),
        (false, true) => Check::fail(
            "spa_built",
            "web UI in the binary",
            "this binary was built without the web UI, so port 9990 serves an error page; install a release build",
            Some(SetupStep::Service),
        ),
    }
}

fn sidecar(facts: &SidecarFacts) -> Vec<Check> {
    let reachable = match &facts.reachable {
        Ok(status) => Check::ok(
            "sidecar",
            "forward sidecar",
            format!("{} answered HTTP {status}", facts.url),
        ),
        Err(why) => Check::warn(
            "sidecar",
            "forward sidecar",
            format!(
                "not reachable: {why}; sessions are captured without the Remote Control channel"
            ),
            Some(SetupStep::Service),
        ),
    };
    let ca = match &facts.ca_found {
        Some(path) => Check::ok(
            "sidecar_ca",
            "sidecar certificate",
            path.display().to_string(),
        ),
        None => Check::warn(
            "sidecar_ca",
            "sidecar certificate",
            format!(
                "no CA certificate at {}; `mike forward` creates it on first start",
                facts
                    .ca_candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            Some(SetupStep::Service),
        ),
    };
    vec![reachable, ca]
}

fn claude(facts: &ToolFacts) -> Check {
    match (&facts.path, &facts.version) {
        (None, _) => Check::fail(
            "claude",
            "claude",
            "`claude` is not on PATH",
            Some(SetupStep::Claude),
        ),
        (Some(path), Some(version)) => Check::ok(
            "claude",
            "claude",
            format!("{} ({version})", path.display()),
        ),
        (Some(path), None) if facts.version_skipped => Check::ok(
            "claude",
            "claude",
            format!("{} (version not read: MIKE_SANDBOX is on)", path.display()),
        ),
        (Some(path), None) => Check::warn(
            "claude",
            "claude",
            format!("{} did not report a version", path.display()),
            Some(SetupStep::Claude),
        ),
    }
}

fn models(facts: &ModelFacts) -> Vec<Check> {
    let mut found = Vec::new();
    if facts.claude_on_path {
        found.push("claude on PATH".to_string());
    }
    if facts.codex_on_path {
        found.push("codex on PATH".to_string());
    }
    if let Some(variable) = &facts.openrouter_key_variable {
        found.push(format!("OpenRouter key in {variable}"));
    }
    let mut checks = vec![if found.is_empty() {
        Check::warn(
            "models",
            "background models",
            "no provider is available: claude and codex are not on PATH and no OpenRouter key is set",
            Some(SetupStep::Models),
        )
    } else {
        Check::ok("models", "background models", found.join(", "))
    }];
    checks.extend(capabilities(facts));
    checks
}

fn capabilities(facts: &ModelFacts) -> Option<Check> {
    let rows = facts.capabilities.as_ref()?;
    let not_active: Vec<String> = rows
        .iter()
        .filter(|r| r.state == "degraded" || r.state == "inert")
        .map(|r| format!("{} is {}", r.label, r.state))
        .collect();
    Some(if not_active.is_empty() {
        Check::ok(
            "capabilities",
            "running capabilities",
            format!("{} reported, none degraded", rows.len()),
        )
    } else {
        Check::warn(
            "capabilities",
            "running capabilities",
            not_active.join("; "),
            Some(SetupStep::Features),
        )
    })
}

fn embeddings(facts: &EmbeddingFacts) -> Check {
    match (&facts.url, facts.reachable) {
        (None, _) => Check::ok(
            "embeddings",
            "embedding server",
            "not configured; semantic recall stays off",
        ),
        (Some(url), Some(true)) => {
            Check::ok("embeddings", "embedding server", format!("{url} answers"))
        }
        (Some(url), Some(false)) => Check::warn(
            "embeddings",
            "embedding server",
            format!("{url} is configured but does not answer"),
            Some(SetupStep::Embeddings),
        ),
        (Some(url), None) => Check::ok(
            "embeddings",
            "embedding server",
            format!("{url} is configured; not probed"),
        ),
    }
}

fn rerank(facts: &RerankFacts) -> Check {
    let Some(url) = &facts.url else {
        return Check::ok(
            "rerank",
            "reranker",
            "not enabled; search results keep their plain order",
        );
    };
    match facts.reachable {
        Some(true) => Check::ok("rerank", "reranker", format!("{url} answers")),
        None => Check::ok(
            "rerank",
            "reranker",
            format!("{url} is enabled; not probed because mike is not running"),
        ),
        Some(false) => {
            let detail = facts
                .detail
                .clone()
                .unwrap_or_else(|| format!("{url} is enabled but does not answer"));
            Check::warn("rerank", "reranker", detail, Some(SetupStep::Rerank))
        }
    }
}

fn launch_backend(backend: &str) -> Check {
    if backend == "none" {
        Check::warn(
            "launch_backend",
            "terminal backend",
            "none chosen, so `mike spawn` and fleets are unavailable",
            Some(SetupStep::Terminal),
        )
    } else {
        Check::ok("launch_backend", "terminal backend", backend.to_string())
    }
}
