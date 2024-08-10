use serde::{Deserialize, Serialize};

use crate::{
    login::UserId,
    schema::Vec2,
    wall::{self, SessionId, WallId},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "login",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LoginRequest {
    New { user: UserId },
    Join { user: UserId, wall: WallId },
}

impl LoginRequest {
    pub fn user_id(&self) -> &UserId {
        match self {
            LoginRequest::New { user } => user,
            LoginRequest::Join { user, .. } => user,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Online {
    pub session_id: SessionId,
    pub nickname: String,
    pub cursor: Option<Vec2>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallInfo {
    pub chunk_size: u32,
    pub online: Vec<Online>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "response",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LoginResponse {
    LoggedIn {
        wall: WallId,
        wall_info: WallInfo,
        session_id: SessionId,
    },
    UserDoesNotExist,
    TooManySessions,
}
