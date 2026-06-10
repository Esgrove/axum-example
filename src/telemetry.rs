//! OpenTelemetry metric export.
//!
//! A Prometheus reader is always attached so `/metrics` can scrape the same
//! instruments that optional OTLP export uses.
//! OTLP push export is enabled only when an OTLP endpoint env var is present.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, Histogram, MeterProvider as _};
use opentelemetry_otlp::MetricExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Encoder, Registry, TextEncoder};

use crate::log_info;
use crate::log_warn;

/// Environment variable that points the app at an OTLP collector endpoint.
pub const OTEL_EXPORTER_OTLP_ENDPOINT_ENV_VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Signal-specific endpoint override for OTLP metrics.
pub const OTEL_EXPORTER_OTLP_METRICS_ENDPOINT_ENV_VAR: &str = "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT";

const INSTRUMENTATION_SCOPE: &str = "axum-example";
const SERVICE_NAME: &str = "axum-example";
const UNKNOWN_METHOD: &str = "unknown";
const UNKNOWN_ROUTE: &str = "unknown";

/// OpenTelemetry provider lifecycle plus shared metric instruments.
#[derive(Debug)]
pub struct Telemetry {
    provider: SdkMeterProvider,
    registry: Registry,
    metrics: Arc<TelemetryMetrics>,
}

/// OpenTelemetry instruments used by the example API.
#[derive(Clone, Debug)]
pub struct TelemetryMetrics {
    requests_started: Counter<u64>,
    requests_completed: Counter<u64>,
    request_duration_ms: Histogram<u64>,
    in_progress_requests: Gauge<u64>,
    errors: Counter<u64>,
}

/// Fields recorded when a request completes.
#[derive(Debug, Clone, Copy)]
pub struct CompletedRequestMetric<'a> {
    /// Low-cardinality route label.
    pub route: &'a str,
    /// HTTP method used by the inbound request.
    pub method: &'a str,
    /// HTTP status returned to the caller.
    pub status: u16,
    /// End-to-end request latency observed by the middleware.
    pub latency: Duration,
    /// Current in-flight request count after this request completed.
    pub in_progress: u64,
}

impl Telemetry {
    /// Initialize OpenTelemetry metrics from environment configuration.
    ///
    /// Prometheus scrape export is always initialized for `/metrics`.
    /// OTLP push export is added only when `OTEL_EXPORTER_OTLP_ENDPOINT`
    /// or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` is set.
    ///
    /// # Errors
    ///
    /// Returns an error when an exporter cannot be built.
    pub fn from_env() -> Result<Self> {
        Self::new(otlp_metrics_enabled())
    }

    /// Build a telemetry handle without OTLP push export.
    #[must_use]
    pub fn noop() -> Self {
        Self::new(false).expect("Prometheus telemetry should build")
    }

    /// Return the shared metric instruments.
    #[must_use]
    pub fn metrics(&self) -> Arc<TelemetryMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Render the current OpenTelemetry metrics as Prometheus text.
    ///
    /// # Errors
    ///
    /// Returns an error when encoding the gathered metric families fails.
    pub fn render_prometheus(&self) -> Result<(String, String)> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder
            .encode(&metric_families, &mut buffer)
            .context("failed to encode Prometheus metrics")?;
        let body = String::from_utf8(buffer).context("Prometheus encoder returned non-UTF-8 data")?;
        Ok((body, encoder.format_type().to_string()))
    }

    /// Flush and shut down the OpenTelemetry provider.
    pub fn shutdown(&self) {
        if let Err(err) = self.provider.shutdown() {
            log_warn!(error = %err, "failed to shut down OpenTelemetry metrics provider");
        }
    }

    fn new(enable_otlp: bool) -> Result<Self> {
        let registry = Registry::new();
        let prometheus_exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .without_counter_suffixes()
            .build()
            .context("failed to build Prometheus metric exporter")?;

        let mut provider_builder = SdkMeterProvider::builder()
            .with_reader(prometheus_exporter)
            .with_resource(Resource::builder().with_service_name(SERVICE_NAME).build());

        if enable_otlp {
            let otlp_exporter = MetricExporter::builder()
                .with_tonic()
                .build()
                .context("failed to build OTLP metric exporter")?;
            provider_builder = provider_builder.with_periodic_exporter(otlp_exporter);
            log_info!("OpenTelemetry OTLP metrics enabled");
        }

        let provider = provider_builder.build();
        let meter = provider.meter(INSTRUMENTATION_SCOPE);
        let metrics = Arc::new(TelemetryMetrics::new(&meter));

        Ok(Self {
            provider,
            registry,
            metrics,
        })
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::noop()
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl TelemetryMetrics {
    /// Build all metric instruments from the supplied meter.
    #[must_use]
    pub fn new(meter: &opentelemetry::metrics::Meter) -> Self {
        Self {
            requests_started: meter
                .u64_counter("axum_example_http_requests_started_total")
                .with_description("HTTP requests that started.")
                .with_unit("1")
                .build(),
            requests_completed: meter
                .u64_counter("axum_example_http_requests_completed_total")
                .with_description("HTTP requests that completed.")
                .with_unit("1")
                .build(),
            request_duration_ms: meter
                .u64_histogram("axum_example_http_request_duration_ms")
                .with_description("HTTP request duration.")
                .with_unit("ms")
                .build(),
            in_progress_requests: meter
                .u64_gauge("axum_example_http_in_progress_requests")
                .with_description("HTTP requests currently in progress.")
                .with_unit("1")
                .build(),
            errors: meter
                .u64_counter("axum_example_http_errors_total")
                .with_description("Completed HTTP requests with error status codes.")
                .with_unit("1")
                .build(),
        }
    }

    /// Record an HTTP request starting.
    pub fn record_request_started(&self, route: &str, method: &str, in_progress: u64) {
        let attributes = request_attributes(route, method);
        self.requests_started.add(1, &attributes);
        self.in_progress_requests.record(in_progress, &attributes);
    }

    /// Record an HTTP request completing.
    pub fn record_request_completed(&self, metric: CompletedRequestMetric<'_>) {
        let attributes = completed_request_attributes(metric.route, metric.method, metric.status);
        let in_progress_attributes = request_attributes(metric.route, metric.method);
        self.requests_completed.add(1, &attributes);
        self.request_duration_ms.record(
            u64::try_from(metric.latency.as_millis()).unwrap_or(u64::MAX),
            &attributes,
        );
        self.in_progress_requests
            .record(metric.in_progress, &in_progress_attributes);

        if metric.status >= 400 {
            self.errors.add(
                1,
                &[
                    KeyValue::new("route", normalized_route(metric.route)),
                    KeyValue::new("method", normalized_method(metric.method)),
                    KeyValue::new("status_class", status_class(metric.status)),
                    KeyValue::new("status_code", i64::from(metric.status)),
                ],
            );
        }
    }
}

fn otlp_metrics_enabled() -> bool {
    env::var_os(OTEL_EXPORTER_OTLP_METRICS_ENDPOINT_ENV_VAR).is_some()
        || env::var_os(OTEL_EXPORTER_OTLP_ENDPOINT_ENV_VAR).is_some()
}

fn request_attributes(route: &str, method: &str) -> [KeyValue; 2] {
    [
        KeyValue::new("route", normalized_route(route)),
        KeyValue::new("method", normalized_method(method)),
    ]
}

fn completed_request_attributes(route: &str, method: &str, status: u16) -> [KeyValue; 4] {
    [
        KeyValue::new("route", normalized_route(route)),
        KeyValue::new("method", normalized_method(method)),
        KeyValue::new("status_class", status_class(status)),
        KeyValue::new("status_code", i64::from(status)),
    ]
}

const fn status_class(status: u16) -> &'static str {
    match status {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "unknown",
    }
}

fn normalized_method(method: &str) -> String {
    if method.is_empty() {
        return UNKNOWN_METHOD.to_string();
    }
    method.to_ascii_uppercase()
}

fn normalized_route(route: &str) -> String {
    if route.is_empty() {
        return UNKNOWN_ROUTE.to_string();
    }
    route.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_class_labels_every_status_family() {
        for (status, expected) in [
            (101, "1xx"),
            (204, "2xx"),
            (302, "3xx"),
            (404, "4xx"),
            (503, "5xx"),
            (700, "unknown"),
        ] {
            assert_eq!(status_class(status), expected);
        }
    }

    #[test]
    fn normalizes_empty_and_lowercase_values() {
        assert_eq!(normalized_method(""), UNKNOWN_METHOD);
        assert_eq!(normalized_method("post"), "POST");
        assert_eq!(normalized_route(""), UNKNOWN_ROUTE);
        assert_eq!(normalized_route("/items"), "/items");
    }

    #[test]
    fn renders_prometheus_metrics_from_shared_instruments() {
        let telemetry = Telemetry::noop();
        let metrics = telemetry.metrics();

        metrics.record_request_started("/items", "post", 1);
        metrics.record_request_completed(CompletedRequestMetric {
            route: "/items",
            method: "post",
            status: 201,
            latency: Duration::from_millis(25),
            in_progress: 0,
        });

        let (body, content_type) = telemetry.render_prometheus().expect("metrics render");

        assert!(content_type.contains("text/plain"));
        assert!(body.contains("axum_example_http_requests_started_total"));
        assert!(body.contains("axum_example_http_requests_completed_total"));
        assert!(body.contains("axum_example_http_request_duration_ms"));
        assert!(body.contains("route=\"/items\""));
        assert!(body.contains("method=\"POST\""));
    }

    #[test]
    fn records_error_metrics_for_error_status_codes() {
        let telemetry = Telemetry::noop();
        let metrics = telemetry.metrics();

        metrics.record_request_started("", "", 1);
        metrics.record_request_completed(CompletedRequestMetric {
            route: "",
            method: "",
            status: 503,
            latency: Duration::from_millis(10),
            in_progress: 0,
        });

        let (body, _) = telemetry.render_prometheus().expect("metrics render");

        assert!(body.contains("axum_example_http_errors_total"));
        assert!(body.contains("route=\"unknown\""));
        assert!(body.contains("method=\"unknown\""));
        assert!(body.contains("status_class=\"5xx\""));
        assert!(body.contains("status_code=\"503\""));
    }
}
