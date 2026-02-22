mod db;
mod handlers;
mod models;
mod routes;
mod telemetry;

use std::env;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let tracer_provider = telemetry::init_telemetry();

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

    let _ = tracer_provider.shutdown();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, flushing telemetry...");
}
