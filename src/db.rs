use anyhow::Context;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> anyhow::Result<PgPool> {
    PgPool::connect(database_url)
        .await
        .context("Failed to connect to DB")
}
