use opentelemetry::metrics::Counter;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub users_created_counter: Counter<u64>,
}
