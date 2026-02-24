use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};

pub fn init_provider() -> SdkTracerProvider {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create OTLP exporter");

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("rust-telemetry").build())
        .build()
}
