use axum::{Router, routing::{get, post}};
use sqlx::PgPool;

use crate::handlers::{add_user, get_user, get_users};

pub fn create_router(pool: PgPool) -> Router {
    Router::new()
        .route("/user/{id}", get(get_user))
        .route("/users", get(get_users))
        .route("/user", post(add_user))
        .with_state(pool)
}
