mod db;
mod handlers;
mod models;
mod otel;
mod routes;

use opentelemetry::trace::TracerProvider;
use std::env;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    let provider = otel::init_provider();

    let tracer = provider.tracer("rust-telemetry");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
	                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = db::create_pool(&database_url).await;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Connected to database and migrations applied");

    let app = routes::create_router(pool);
    let listener = TcpListener::bind("0.0.0.0:3000").await.expect("Failed to bind");
    tracing::info!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    let _ = provider.shutdown();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, flushing telemetry...");
}
