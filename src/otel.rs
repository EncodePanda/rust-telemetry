use anyhow::Context;
use opentelemetry_otlp::{MetricExporter, SpanExporter};
use opentelemetry_sdk::{Resource, metrics::SdkMeterProvider, trace::SdkTracerProvider};

pub struct Providers {
    pub tracer: SdkTracerProvider,
    pub meter: SdkMeterProvider,
}

pub fn init_providers() -> anyhow::Result<Providers> {
    let resource = Resource::builder().with_service_name("rust-telemetry").build();

    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .context("Failed to create OTLP span exporter")?;

    let tracer = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();

    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .build()
        .context("Failed to create OTLP metric exporter")?;

    let meter = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();

    Ok(Providers { tracer, meter })
}
