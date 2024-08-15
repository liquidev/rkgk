use serde::{Deserialize, Serialize};

use crate::{
    login::UserId,
    schema::Vec2,
    wall::{self, ChunkPosition, SessionId, UserInit, WallId},
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
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub user: UserId,
    /// If null, a new wall is created.
    pub wall: Option<WallId>,
    pub init: UserInit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Online {
    pub session_id: SessionId,
    pub nickname: String,
    pub cursor: Option<Vec2>,
    pub init: UserInit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallInfo {
    pub chunk_size: u32,
    pub paint_area: u32,
    pub haku_limits: crate::haku::Limits,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "request",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    Wall {
        wall_event: wall::EventKind,
    },

    Viewport {
        top_left: ChunkPosition,
        bottom_right: ChunkPosition,
    },

    MoreChunks,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChunkInfo {
    pub position: ChunkPosition,
    pub offset: u32,
    pub length: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "notify",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Notify {
    Wall { wall_event: wall::Event },
    Chunks { chunks: Vec<ChunkInfo> },
}
