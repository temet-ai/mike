use anyhow::Context;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{self, ProxyConfig};

type OtelLayer = tracing_opentelemetry::OpenTelemetryLayer<
    tracing_subscriber::Registry,
    opentelemetry_sdk::trace::SdkTracer,
>;

pub(super) struct TracingGuards {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    pub(super) otel_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

pub(super) fn init_tracing_and_log_startup(config: &ProxyConfig) -> anyhow::Result<TracingGuards> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    let (file_guard, file_writer_opt) = build_file_writer(config)?;
    let (otel_layer, otel_provider) = build_otel_layer(config)?;

    install_subscriber(config, env_filter, otel_layer, file_writer_opt);

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
    env_filter: tracing_subscriber::EnvFilter,
    otel_layer: Option<OtelLayer>,
    file_writer_opt: Option<tracing_appender::non_blocking::NonBlocking>,
) {
    if config.log_format == "json" {
        let file_layer =
            file_writer_opt.map(|w| tracing_subscriber::fmt::layer().json().with_writer(w));
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .with(file_layer)
            .init();
    } else {
        let file_layer =
            file_writer_opt.map(|w| tracing_subscriber::fmt::layer().json().with_writer(w));
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(env_filter)
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
        "Set ANTHROPIC_BASE_URL=http://{} to route Claude through this proxy",
        config.proxy_listen_addr
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
    tracing::warn!("OpenTelemetry shutdown error: {message}");
}

fn log_otel_shutdown_ok() {
    info!("OpenTelemetry provider shut down");
}
