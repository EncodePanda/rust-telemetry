use axum::{Router, routing::{get, post}};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use sqlx::PgPool;

use crate::handlers::{add_user, get_user, get_users};

pub fn create_router(pool: PgPool) -> Router {
    Router::new()
        .route("/user/{id}", get(get_user))
        .route("/users", get(get_users))
        .route("/user", post(add_user))
        .layer(OtelInResponseLayer::default())
        .layer(OtelAxumLayer::default())
        .with_state(pool)
}
