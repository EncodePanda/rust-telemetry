use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use sqlx::Row;
use tracing::{Instrument, instrument};
use uuid::Uuid;

use crate::models::{CreateUserRequest, User};
use crate::state::AppState;

#[instrument(skip(state))]
pub async fn get_users(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, first_name, last_name FROM users")
        .fetch_all(&state.db)
        .instrument(tracing::info_span!("db.query", db.statement = "SELECT users"))
        .await
        .expect("Query failed");

    let users: Vec<User> = {
        let _span = tracing::info_span!("result.map", row_count = rows.len()).entered();
        rows.iter()
            .map(|row| User {
                id: row.get("id"),
                first_name: row.get("first_name"),
                last_name: row.get("last_name"),
            })
            .collect()
    };

    Json(users)
}

#[instrument(skip(state), fields(user_id = %id))]
pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query("SELECT id, first_name, last_name FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .instrument(tracing::info_span!("db.query", db.statement = "SELECT user BY id"))
        .await
        .expect("Query failed");

    let _span = tracing::info_span!("result.build").entered();
    match row {
        Some(row) => {
            let user = User {
                id: row.get("id"),
                first_name: row.get("first_name"),
                last_name: row.get("last_name"),
            };
            (StatusCode::OK, Json(Some(user))).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[instrument(skip(state, body), fields(user_first_name = %body.first_name))]
pub async fn add_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO users (id, first_name, last_name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&body.first_name)
        .bind(&body.last_name)
        .execute(&state.db)
        .instrument(tracing::info_span!("db.query", db.statement = "INSERT user"))
        .await
        .expect("Insert failed");

    state.users_created_counter.add(1, &[]);

    let user = {
        let _span = tracing::info_span!("result.build").entered();
        User {
            id,
            first_name: body.first_name,
            last_name: body.last_name,
        }
    };

    (StatusCode::CREATED, Json(user))
}
