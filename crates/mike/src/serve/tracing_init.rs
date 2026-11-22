use anyhow::Context;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{self, ProxyConfig};

type OtelLayer = tracing_opentelemetry::OpenTelemetryLayer<
    tracing_subscriber::Registry,
    opentelemetry_sdk::trace::SdkTracer,
>;

type FilterSubscriber =
    tracing_subscriber::layer::Layered<Option<OtelLayer>, tracing_subscriber::Registry>;

type FilterLayer =
    tracing_subscriber::reload::Layer<tracing_subscriber::EnvFilter, FilterSubscriber>;

type FilterHandle =
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, FilterSubscriber>;

static FILTER_HANDLE: std::sync::OnceLock<FilterHandle> = std::sync::OnceLock::new();

pub(super) struct TracingGuards {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    pub(super) otel_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

pub(crate) fn set_log_level(level: &str) {
    let Some(handle) = FILTER_HANDLE.get() else {
        return;
    };
    if let Err(error) = reload_filter(handle, level) {
        log_level_reload_failed(&error, level);
    }
}

/// The `notify` crate logs a WARN for every watch the kernel already dropped when a
/// watched directory is deleted (a worktree removed under the change-record watcher):
/// a normal event, not a problem, so only its errors are kept.
const QUIET_TARGETS: &str = "notify::inotify=error";

fn directives(level: &str) -> String {
    format!("{level},{QUIET_TARGETS}")
}

fn reload_filter(handle: &FilterHandle, level: &str) -> Result<(), String> {
    let filter =
        tracing_subscriber::EnvFilter::try_new(directives(level)).map_err(|e| e.to_string())?;
    handle.reload(filter).map_err(|e| e.to_string())
}

fn log_level_reload_failed(error: &str, level: &str) {
    tracing::warn!(error, level, "log level reload failed");
}

fn build_filter_layer(config: &ProxyConfig) -> FilterLayer {
    let from_env = tracing_subscriber::EnvFilter::try_from_default_env();
    let rust_log_set = from_env.is_ok();
    let filter = from_env
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(directives(&config.log_level)));
    let (layer, handle) = tracing_subscriber::reload::Layer::new(filter);
    if !rust_log_set {
        let _already_installed = FILTER_HANDLE.set(handle);
    }
    layer
}

pub(super) fn init_tracing_and_log_startup(config: &ProxyConfig) -> anyhow::Result<TracingGuards> {
    let filter_layer = build_filter_layer(config);

    let (file_guard, file_writer_opt) = build_file_writer(config)?;
    let (otel_layer, otel_provider) = build_otel_layer(config)?;

    install_subscriber(config, filter_layer, otel_layer, file_writer_opt);

    log_boot_configuration(config);

    Ok(TracingGuards {
        _file_guard: file_guard,
        otel_provider,
    })
}

fn build_file_writer(
    config: &ProxyConfig,
) -> anyhow::Result<(
    Option<tracing_appender::non_blocking::WorkerGuard>,
    Option<tracing_appender::non_blocking::NonBlocking>,
)> {
    let Some(dir) = &config.log_dir else {
        return Ok((None, None));
    };
    crate::config::write::ensure_private_directory(dir)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("mike")
        .filename_suffix("log")
        .max_log_files(config.log_max_files)
        .build(dir)
        .with_context(|| format!("failed to build file log appender in {}", dir.display()))?;
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);
    Ok((Some(guard), Some(non_blocking)))
}

fn build_otel_layer(
    config: &ProxyConfig,
) -> anyhow::Result<(
    Option<OtelLayer>,
    Option<opentelemetry_sdk::trace::SdkTracerProvider>,
)> {
    let Some(endpoint) = &config.otel_endpoint else {
        return Ok((None, None));
    };
    let (layer, provider) =
        init_otel_layer(endpoint).context("failed to initialize OpenTelemetry")?;
    Ok((Some(layer), Some(provider)))
}

fn install_subscriber(
    config: &ProxyConfig,
    filter_layer: FilterLayer,
    otel_layer: Option<OtelLayer>,
    file_writer_opt: Option<tracing_appender::non_blocking::NonBlocking>,
) {
    if config.log_format == "json" {
        let file_layer =
            file_writer_opt.map(|w| tracing_subscriber::fmt::layer().json().with_writer(w));
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(filter_layer)
            .with(tracing_subscriber::fmt::layer().json())
            .with(file_layer)
            .init();
    } else {
        let file_layer =
            file_writer_opt.map(|w| tracing_subscriber::fmt::layer().json().with_writer(w));
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(filter_layer)
            .with(tracing_subscriber::fmt::layer())
            .with(file_layer)
            .init();
    }
}

fn log_boot_configuration(config: &ProxyConfig) {
    log_mike_starting(config);
    log_resolved_capture_database_target(config);
    log_anthropic_base_url_hint(config);
}

fn log_mike_starting(config: &ProxyConfig) {
    info!(
        proxy_listen_addr = %config.proxy_listen_addr,
        inspect_listen_addr = %config.inspect_listen_addr,
        upstream_url = %config.upstream_url,
        "mike starting"
    );
}

fn log_resolved_capture_database_target(config: &ProxyConfig) {
    info!(
        database_url = %config
            .database_url
            .as_deref()
            .map(config::load::redact_database_url)
            .unwrap_or_else(|| "<unset>".to_string()),
        local_database_url = %config
            .local_database_url
            .as_deref()
            .map(config::load::redact_database_url)
            .unwrap_or_else(|| "<unset>".to_string()),
        "resolved capture database target"
    );
}

fn log_anthropic_base_url_hint(config: &ProxyConfig) {
    info!(
        anthropic_base_url = %format_args!("http://{}", config.proxy_listen_addr),
        "set ANTHROPIC_BASE_URL to this value to route Claude through this proxy"
    );
}

fn init_otel_layer(
    endpoint: &str,
) -> anyhow::Result<(OtelLayer, opentelemetry_sdk::trace::SdkTracerProvider)> {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build OTLP exporter: {e}"))?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("mike")
                .build(),
        )
        .build();

    let tracer = provider.tracer("mike");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((layer, provider))
}

pub(super) fn flush_otel(otel_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>) {
    let Some(provider) = otel_provider else {
        return;
    };
    if let Err(e) = provider.shutdown() {
        log_otel_shutdown_failed(&e.to_string());
    } else {
        log_otel_shutdown_ok();
    }
}

fn log_otel_shutdown_failed(message: &str) {
    tracing::warn!(error = %message, "OpenTelemetry shutdown failed");
}

fn log_otel_shutdown_ok() {
    info!("OpenTelemetry provider shut down");
}

#[cfg(test)]
mod quiet_targets_tests {
    use super::directives;

    #[test]
    fn a_level_keeps_notify_inotify_at_error() {
        assert_eq!(directives("info"), "info,notify::inotify=error");
        assert!(tracing_subscriber::EnvFilter::try_new(directives("debug")).is_ok());
    }
}
