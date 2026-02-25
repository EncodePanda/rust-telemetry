use axum::{Router, routing::{get, post}};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};

use crate::handlers::{add_user, get_user, get_users};
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/user/{id}", get(get_user))
        .route("/users", get(get_users))
        .route("/user", post(add_user))
        .layer(OtelInResponseLayer::default())
        .layer(OtelAxumLayer::default())
        .with_state(state)
}
