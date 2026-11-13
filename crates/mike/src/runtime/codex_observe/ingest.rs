use crate::store::persistence::runtime_repo::RuntimePersist as _;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::{home, identity, session};
use crate::runtime::{RuntimeKind, RuntimeNamespace};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime::ObservationApplyOutcome;
use mike_codex::observe::{HookEvent, HookPayload, TelemetryFact, log_facts, resource_version};

const NAMESPACE_SEED: Uuid = Uuid::from_u128(0x6d69_6b65_636f_6465_786f_6273_6572_7665);

#[derive(Debug, Clone, Default)]
pub(crate) struct ObserveSource {
    pub label: String,
    pub codex_version: Option<String>,
    pub hook_boundary: bool,
    pub codex_home: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct IngestReport {
    pub namespace_id: Uuid,
    pub ingestion_id: Uuid,
    pub replayed: bool,
    pub facts: usize,
    pub control: &'static str,
    pub mike_session_id: Option<Uuid>,
    pub pending_mail: Vec<Uuid>,
    pub availability: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum IngestError {
    #[error("{0}")]
    BadInput(String),
    #[error("{0}")]
    Store(String),
}

impl From<crate::store::persistence::PersistError> for IngestError {
    fn from(error: crate::store::persistence::PersistError) -> Self {
        Self::Store(error.to_string())
    }
}

pub(crate) fn namespace_for(source: &ObserveSource) -> RuntimeNamespace {
    let durable_key = format!("codex-observed:{}", source.label);
    RuntimeNamespace {
        id: Uuid::new_v5(&NAMESPACE_SEED, durable_key.as_bytes()),
        runtime: RuntimeKind::Codex,
        durable_key,
    }
}

pub(crate) fn otel_ingestion_id(namespace: &RuntimeNamespace, signal: &str, raw: &[u8]) -> Uuid {
    let mut name = signal.as_bytes().to_vec();
    name.push(0);
    name.extend_from_slice(raw);
    Uuid::new_v5(&namespace.id, &name)
}

pub(crate) async fn ingest_hook(
    persist: &PersistHandle,
    source: &ObserveSource,
    delivery_id: Uuid,
    raw: &[u8],
) -> Result<IngestReport, IngestError> {
    validate_size(raw)?;
    let payload = HookPayload::parse(raw).map_err(IngestError::BadInput)?;
    let namespace = source_namespace(persist, source).await?;
    let scope = payload.scope();
    let parent = scope.parent_thread_id.as_deref();
    let session = resolve_hook_session(persist, &namespace, parent, &scope.thread_id).await?;
    let observation = session::hook(persist, source, session, delivery_id, &payload).await?;
    sync_lifecycle(persist, session, delivery_id, &payload).await?;
    let outcome = persist
        .runtime()
        .apply_session_observation(&observation)
        .await?;
    let mut report = report(namespace.id, delivery_id, outcome);
    report.mike_session_id = Some(session);
    let boundary = source.hook_boundary && parent.is_none();
    report.control = if boundary {
        "hook_boundary"
    } else {
        "pull_only"
    };
    if boundary && outcome_applied_or_replayed(outcome) {
        crate::mail_outbox::observe_local_codex(
            persist,
            session,
            &scope.thread_id,
            source.codex_home.as_deref(),
        )
        .await;
    }
    if source.hook_boundary && !report.replayed {
        report.pending_mail = persist.runtime().pending_runtime_mail(session, 20).await?;
    }
    Ok(report)
}

fn outcome_applied_or_replayed(outcome: ObservationApplyOutcome) -> bool {
    matches!(
        outcome,
        ObservationApplyOutcome::Applied | ObservationApplyOutcome::Replayed
    )
}

async fn resolve_hook_session(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    parent: Option<&str>,
    thread: &str,
) -> Result<Uuid, IngestError> {
    match parent {
        Some(parent) => identity::resolve_child(persist, namespace, parent, thread).await,
        None => identity::resolve(persist, namespace, thread).await,
    }
}

async fn sync_lifecycle(
    persist: &PersistHandle,
    session: Uuid,
    delivery_id: Uuid,
    payload: &HookPayload,
) -> Result<(), IngestError> {
    let replayed = persist
        .runtime()
        .session_observation_time(session, delivery_id)
        .await?
        .is_some();
    if replayed {
        return Ok(());
    }
    if matches!(
        payload.event(),
        HookEvent::SessionEnd | HookEvent::SubagentStop
    ) {
        identity::end(persist, session).await?;
    } else {
        identity::register(persist, session, payload.cwd.as_deref()).await?;
    }
    Ok(())
}

fn report(
    namespace_id: Uuid,
    ingestion_id: Uuid,
    outcome: ObservationApplyOutcome,
) -> IngestReport {
    IngestReport {
        namespace_id,
        ingestion_id,
        replayed: outcome == ObservationApplyOutcome::Replayed,
        facts: usize::from(outcome == ObservationApplyOutcome::Applied),
        control: "pull_only",
        mike_session_id: None,
        pending_mail: Vec::new(),
        availability: match outcome {
            ObservationApplyOutcome::Applied | ObservationApplyOutcome::Replayed => "metadata_only",
            ObservationApplyOutcome::Expired => "expired",
            ObservationApplyOutcome::BudgetRefused => "budget_refused",
        },
    }
}

pub(crate) async fn ingest_otel(
    persist: &PersistHandle,
    source: &ObserveSource,
    signal: &str,
    raw: &[u8],
) -> Result<IngestReport, IngestError> {
    validate_size(raw)?;
    let body: Value = serde_json::from_slice(raw).map_err(|error| {
        IngestError::BadInput(format!(
            "OTLP {signal} body is not JSON at line {}, column {}: {error}; set protocol = \"json\" on the Codex otlp-http exporter",
            error.line(), error.column()
        ))
    })?;
    let mut source = source.clone();
    source.codex_version = source.codex_version.or_else(|| resource_version(&body));
    let namespace = source_namespace(persist, &source).await?;
    let ingestion = otel_ingestion_id(&namespace, signal, raw);
    let mut report = report(namespace.id, ingestion, ObservationApplyOutcome::Applied);
    report.facts = 0;
    report.replayed = true;
    if signal != "logs" {
        report.availability = "signal_not_projected";
        return Ok(report);
    }
    for (index, fact) in log_facts(&body).iter().enumerate() {
        let Some(thread) = session::thread(fact) else {
            continue;
        };
        let id = match fact {
            TelemetryFact::Spawn {
                parent_thread_id, ..
            } => identity::resolve_child(persist, &namespace, parent_thread_id, thread).await?,
            _ => identity::resolve(persist, &namespace, thread).await?,
        };
        let delivery = Uuid::new_v5(&ingestion, &index.to_be_bytes());
        let mut observation = session::telemetry(&source, id, delivery, fact);
        if let Some(at) = persist
            .runtime()
            .session_observation_time(id, observation.observation_id)
            .await?
        {
            observation.observed_at = at;
        }
        let outcome = persist
            .runtime()
            .apply_session_observation(&observation)
            .await?;
        report.facts += usize::from(outcome == ObservationApplyOutcome::Applied);
        report.replayed &= outcome == ObservationApplyOutcome::Replayed;
        match outcome {
            ObservationApplyOutcome::BudgetRefused => report.availability = "budget_refused",
            ObservationApplyOutcome::Expired if report.availability != "budget_refused" => {
                report.availability = "expired"
            }
            _ => {}
        }
    }
    Ok(report)
}

fn validate_size(raw: &[u8]) -> Result<(), IngestError> {
    if raw.len() > 512 * 1024 {
        return Err(IngestError::BadInput(
            "Codex observation exceeds the 512 KiB ingest limit".into(),
        ));
    }
    Ok(())
}

async fn source_namespace(
    persist: &PersistHandle,
    source: &ObserveSource,
) -> Result<RuntimeNamespace, IngestError> {
    if let Some(home) = source.codex_home.as_deref()
        && let Some(namespace) =
            home::verified_namespace(persist, std::path::Path::new(home)).await?
    {
        return Ok(namespace);
    }
    Ok(namespace_for(source))
}
