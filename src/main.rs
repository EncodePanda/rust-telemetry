mod db;
mod handlers;
mod models;
mod otel;
mod routes;
mod state;

use anyhow::Context;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::trace::TracerProvider;
use std::env;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let providers = otel::init_providers().context("Failed to initialize telemetry providers")?;

    let tracer = providers.tracer.tracer("rust-telemetry");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
	                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = db::create_pool(&database_url).await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    tracing::info!("Connected to database and migrations applied");

    let meter = providers.meter.meter("rust-telemetry");

    let users_created_counter = meter.u64_counter("app.users.created").build();

    let gauge_pool = pool.clone();
    let _pool_gauge = meter
        .u64_observable_gauge("db.client.connections.pool_size")
        .with_callback(move |observer| {
            observer.observe(gauge_pool.size() as u64, &[]);
        })
        .build();

    let state = AppState {
        db: pool,
        users_created_counter,
    };

    let app = routes::create_router(state);
    let listener = TcpListener::bind("0.0.0.0:3000").await.context("Failed to bind")?;
    tracing::info!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    let _ = providers.tracer.shutdown();
    let _ = providers.meter.shutdown();

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, flushing telemetry...");
}
