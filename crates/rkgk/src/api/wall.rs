use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use eyre::{bail, Context, OptionExt};
use schema::{Error, LoginRequest, LoginResponse, Online, Version, WallInfo};
use serde::{Deserialize, Serialize};
use tokio::select;
use tracing::{error, info};

use crate::{
    login::database::LoginStatus,
    wall::{Event, JoinError, Session},
    Databases,
};

mod schema;

pub async fn wall(State(dbs): State<Arc<Databases>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|ws| websocket(dbs, ws))
}

fn to_message<T>(value: &T) -> Message
where
    T: Serialize,
{
    Message::Text(serde_json::to_string(value).expect("cannot serialize response to JSON"))
}

fn from_message<'de, T>(message: &'de Message) -> eyre::Result<T>
where
    T: Deserialize<'de>,
{
    match message {
        Message::Text(json) => {
            serde_json::from_str(json).context("could not deserialize JSON text message")
        }
        _ => bail!("expected a text message"),
    }
}

async fn recv_expect(ws: &mut WebSocket) -> eyre::Result<Message> {
    Ok(ws
        .recv()
        .await
        .ok_or_eyre("connection closed unexpectedly")??)
}

async fn websocket(dbs: Arc<Databases>, mut ws: WebSocket) {
    match fallible_websocket(dbs, &mut ws).await {
        Ok(()) => (),
        Err(e) => {
            _ = ws
                .send(to_message(&Error {
                    error: format!("{e:?}"),
                }))
                .await
        }
    }
}

async fn fallible_websocket(dbs: Arc<Databases>, ws: &mut WebSocket) -> eyre::Result<()> {
    #[cfg(debug_assertions)]
    let version = format!("{}-dev", env!("CARGO_PKG_VERSION"));
    #[cfg(not(debug_assertions))]
    let version = format!("{}", env!("CARGO_PKG_VERSION"));

    ws.send(to_message(&Version { version })).await?;

    let login_request: LoginRequest = from_message(&recv_expect(ws).await?)?;
    let user_id = *login_request.user_id();

    match dbs
        .login
        .log_in(user_id)
        .await
        .context("error while logging in")?
    {
        LoginStatus::ValidUser => (),
        LoginStatus::UserDoesNotExist => {
            ws.send(to_message(&LoginResponse::UserDoesNotExist))
                .await?;
            return Ok(());
        }
    }

    let wall_id = match login_request {
        LoginRequest::New { .. } => dbs.wall_broker.generate_id().await,
        LoginRequest::Join { wall, .. } => wall,
    };
    let wall = dbs.wall_broker.open(wall_id);

    let mut session_handle = match wall.join(Session::new(user_id)) {
        Ok(handle) => handle,
        Err(error) => {
            ws.send(to_message(&match error {
                // NOTE: Respond with the same error code, because it doesn't matter to the user -
                // either way the room is way too contended for them to join.
                JoinError::TooManyCurrentSessions => LoginResponse::TooManySessions,
                JoinError::IdsExhausted => LoginResponse::TooManySessions,
            }))
            .await?;
            return Ok(());
        }
    };

    let mut users_online = vec![];
    for online in wall.online() {
        let user_info = match dbs.login.user_info(online.user_id).await {
            Ok(Some(user_info)) => user_info,
            Ok(None) | Err(_) => {
                error!(?online, "could not get info about online user");
                continue;
            }
        };
        users_online.push(Online {
            session_id: online.session_id,
            nickname: user_info.nickname,
            cursor: online.cursor,
        })
    }
    let users_online = users_online;

    ws.send(to_message(&LoginResponse::LoggedIn {
        wall: wall_id,
        wall_info: WallInfo {
            chunk_size: wall.settings().chunk_size,
            online: users_online,
        },
        session_id: session_handle.session_id,
    }))
    .await?;

    loop {
        select! {
            Some(message) = ws.recv() => {
                let kind = from_message(&message?)?;
                wall.event(Event { session_id: session_handle.session_id, kind });
            }

            Ok(event) = session_handle.event_receiver.recv() => {
                ws.send(to_message(&event)).await?;
            }

            else => break,
        }
    }

    Ok(())
}
