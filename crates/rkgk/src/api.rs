use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::Databases;

mod wall;

pub fn router<S>(dbs: Arc<Databases>) -> Router<S> {
    Router::new()
        .route("/login", post(login_new))
        .route("/wall", get(wall::wall))
        .with_state(dbs)
}

#[derive(Deserialize)]
struct NewUserParams {
    nickname: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum NewUserResponse {
    #[serde(rename_all = "camelCase")]
    Ok { user_id: String },

    #[serde(rename_all = "camelCase")]
    Error { message: String },
}

async fn login_new(dbs: State<Arc<Databases>>, params: Json<NewUserParams>) -> impl IntoResponse {
    if !(1..=32).contains(&params.nickname.len()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(NewUserResponse::Error {
                message: "nickname must be 1..=32 characters long".into(),
            }),
        );
    }

    match dbs.login.new_user(params.0.nickname).await {
        Ok(user_id) => (
            StatusCode::OK,
            Json(NewUserResponse::Ok {
                user_id: user_id.to_string(),
            }),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(NewUserResponse::Error {
                message: error.to_string(),
            }),
        ),
    }
}
