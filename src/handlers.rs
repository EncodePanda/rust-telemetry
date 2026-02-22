use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::models::{CreateUserRequest, User};

pub async fn get_users(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let rows = sqlx::query("SELECT id, first_name, last_name FROM users")
        .fetch_all(&pool)
        .await
        .expect("Query failed");

    let users: Vec<User> = rows
        .iter()
        .map(|row| User {
            id: row.get("id"),
            first_name: row.get("first_name"),
            last_name: row.get("last_name"),
        })
        .collect();

    Json(users)
}

pub async fn get_user(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query("SELECT id, first_name, last_name FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .expect("Query failed");

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

pub async fn add_user(
    State(pool): State<PgPool>,
    Json(body): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO users (id, first_name, last_name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&body.first_name)
        .bind(&body.last_name)
        .execute(&pool)
        .await
        .expect("Insert failed");

    let user = User {
        id,
        first_name: body.first_name,
        last_name: body.last_name,
    };

    (StatusCode::CREATED, Json(user))
}
