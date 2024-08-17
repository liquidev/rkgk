use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use eyre::{bail, Context, OptionExt};
use haku::value::Value;
use schema::{
    ChunkInfo, Error, LoginRequest, LoginResponse, Notify, Online, Request, Version, WallInfo,
};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc, time::Instant};
use tracing::{error, info};

use crate::{
    haku::{Haku, Limits},
    login::database::LoginStatus,
    schema::Vec2,
    wall::{
        self, chunk_encoder::ChunkEncoder, chunk_iterator::ChunkIterator, ChunkPosition, JoinError,
        SessionHandle, UserInit, Wall,
    },
};

use super::Api;

mod schema;

pub async fn wall(State(api): State<Arc<Api>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|ws| websocket(api, ws))
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

async fn websocket(api: Arc<Api>, mut ws: WebSocket) {
    match fallible_websocket(api, &mut ws).await {
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

async fn fallible_websocket(api: Arc<Api>, ws: &mut WebSocket) -> eyre::Result<()> {
    #[cfg(debug_assertions)]
    let version = format!("{}-dev", env!("CARGO_PKG_VERSION"));
    #[cfg(not(debug_assertions))]
    let version = format!("{}", env!("CARGO_PKG_VERSION"));

    ws.send(to_message(&Version { version })).await?;

    let login_request: LoginRequest = from_message(&recv_expect(ws).await?)?;
    let user_id = login_request.user;

    match api
        .dbs
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
    let user_info = api
        .dbs
        .login
        .user_info(user_id)
        .await
        .context("cannot get user info")?
        .ok_or_eyre("user seems to have vanished")?;

    let wall_id = match login_request.wall {
        Some(wall) => wall,
        None => api.dbs.wall_broker.generate_id().await,
    };
    let open_wall = api.dbs.wall_broker.open(wall_id);

    let session_handle = match open_wall
        .wall
        .join(wall::Session::new(user_id, login_request.init.clone()))
    {
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
    for online in open_wall.wall.online() {
        let user_info = match api.dbs.login.user_info(online.user_id).await {
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
            init: UserInit {
                brush: online.brush,
            },
        })
    }
    let users_online = users_online;

    ws.send(to_message(&LoginResponse::LoggedIn {
        wall: wall_id,
        wall_info: WallInfo {
            chunk_size: open_wall.wall.settings().chunk_size,
            paint_area: open_wall.wall.settings().paint_area,
            online: users_online,
            haku_limits: api.config.haku.clone(),
        },
        session_id: session_handle.session_id,
    }))
    .await?;

    open_wall.wall.event(wall::Event {
        session_id: session_handle.session_id,
        kind: wall::EventKind::Join {
            nickname: user_info.nickname,
            init: login_request.init.clone(),
        },
    });
    // Leave event is sent in SessionHandle's Drop implementation.
    // This technically means that inbetween the user getting logged in and sending the Join event,
    // we may end up losing the user and sending a Leave event, but Leave events are idempotent -
    // they're only used to clean up the state of an existing user, but the user is not _required_
    // to exist on clients already.
    // ...Well, we'll see how much havoc that wreaks in practice. Especially without TypeScript
    // to remind us about unexpected nulls.

    SessionLoop::start(
        open_wall.wall,
        open_wall.chunk_encoder,
        session_handle,
        api.config.haku.clone(),
        login_request.init.brush,
    )
    .await?
    .event_loop(ws)
    .await?;

    Ok(())
}

struct SessionLoop {
    wall: Arc<Wall>,
    chunk_encoder: Arc<ChunkEncoder>,
    handle: SessionHandle,

    render_commands_tx: mpsc::Sender<RenderCommand>,

    viewport_chunks: ChunkIterator,
    sent_chunks: HashSet<ChunkPosition>,
}

enum RenderCommand {
    SetBrush { brush: String },
    Plot { points: Vec<Vec2> },
}

impl SessionLoop {
    async fn start(
        wall: Arc<Wall>,
        chunk_encoder: Arc<ChunkEncoder>,
        handle: SessionHandle,
        limits: Limits,
        brush: String,
    ) -> eyre::Result<Self> {
        // Limit how many commands may come in _pretty darn hard_ because these can be really
        // CPU-intensive.
        // If this ends up dropping commands - it's your fault for trying to DoS my server!
        let (render_commands_tx, render_commands_rx) = mpsc::channel(1);

        render_commands_tx
            .send(RenderCommand::SetBrush { brush })
            .await
            .unwrap();

        // We spawn our own thread so as not to clog the tokio blocking thread pool with our
        // rendering shenanigans.
        std::thread::Builder::new()
            .name(String::from("haku render thread"))
            .spawn({
                let wall = Arc::clone(&wall);
                let chunk_encoder = Arc::clone(&chunk_encoder);
                move || Self::render_thread(wall, chunk_encoder, limits, render_commands_rx)
            })
            .context("could not spawn render thread")?;

        Ok(Self {
            wall,
            chunk_encoder,
            handle,
            render_commands_tx,
            viewport_chunks: ChunkIterator::new(ChunkPosition::new(0, 0), ChunkPosition::new(0, 0)),
            sent_chunks: HashSet::new(),
        })
    }

    async fn event_loop(&mut self, ws: &mut WebSocket) -> eyre::Result<()> {
        loop {
            select! {
                Some(message) = ws.recv() => {
                    let request = from_message(&message?)?;
                    self.process_request(ws, request).await?;
                }

                Ok(wall_event) = self.handle.event_receiver.recv() => {
                    ws.send(to_message(&Notify::Wall { wall_event })).await?;
                }

                else => break,
            }
        }

        Ok(())
    }

    async fn process_request(&mut self, ws: &mut WebSocket, request: Request) -> eyre::Result<()> {
        match request {
            Request::Wall { wall_event } => {
                match &wall_event {
                    // This match only concerns itself with drawing-related events to offload
                    // all the evaluation and drawing work to this session's drawing thread.
                    wall::EventKind::Join { .. }
                    | wall::EventKind::Leave
                    | wall::EventKind::Cursor { .. } => (),

                    wall::EventKind::SetBrush { brush } => {
                        // SetBrush is not dropped because it is a very important event.
                        _ = self
                            .render_commands_tx
                            .send(RenderCommand::SetBrush {
                                brush: brush.clone(),
                            })
                            .await;
                    }
                    wall::EventKind::Plot { points } => {
                        // We drop commands if we take too long to render instead of lagging
                        // the WebSocket thread.
                        // Theoretically this will yield much better responsiveness, but it _will_
                        // result in some visual glitches if we're getting bottlenecked.
                        _ = self.render_commands_tx.try_send(RenderCommand::Plot {
                            points: points.clone(),
                        })
                    }
                }

                self.wall.event(wall::Event {
                    session_id: self.handle.session_id,
                    kind: wall_event,
                });
            }

            Request::Viewport {
                top_left,
                bottom_right,
            } => {
                self.viewport_chunks = ChunkIterator::new(top_left, bottom_right);
                self.send_chunks(ws).await?;
            }

            Request::MoreChunks => {
                self.send_chunks(ws).await?;
            }
        }

        Ok(())
    }

    async fn send_chunks(&mut self, ws: &mut WebSocket) -> eyre::Result<()> {
        let mut chunk_infos = vec![];
        let mut packet = vec![];

        // Number of chunks iterated is limited per packet, so as not to let the client
        // stall the server by sending in a huge viewport.
        let start = Instant::now();
        let mut iterated = 0;
        for i in 0..12000 {
            if let Some(position) = self.viewport_chunks.next() {
                if self.sent_chunks.contains(&position) {
                    continue;
                }

                if let Some(encoded) = self.chunk_encoder.encoded(position).await {
                    let offset = packet.len();
                    packet.extend_from_slice(&encoded);
                    chunk_infos.push(ChunkInfo {
                        position,
                        offset: u32::try_from(offset).context("packet too big")?,
                        length: u32::try_from(encoded.len()).context("chunk image too big")?,
                    });

                    // The actual number of chunks per packet is limited by the packet's size, which
                    // we don't want to be too big, to maintain responsiveness - the client will
                    // only request more chunks once per frame, so interactions still have time to
                    // execute. We cap it to 256KiB in hopes that noone has Internet slow enough for
                    // this to cause a disconnect.
                    if packet.len() >= 256 * 1024 {
                        iterated = i;
                        break;
                    }
                }

                self.sent_chunks.insert(position);
            } else {
                iterated = i;
                break;
            }
        }
        info!(elapsed = ?start.elapsed(), iterated, "send_chunks");

        ws.send(to_message(&Notify::Chunks {
            chunks: chunk_infos,
            has_more: self.viewport_chunks.clone().next().is_some(),
        }))
        .await?;
        ws.send(Message::Binary(packet)).await?;

        Ok(())
    }

    fn render_thread(
        wall: Arc<Wall>,
        chunk_encoder: Arc<ChunkEncoder>,
        limits: Limits,
        mut commands: mpsc::Receiver<RenderCommand>,
    ) {
        let mut haku = Haku::new(limits);
        let mut brush_ok = false;

        while let Some(command) = commands.blocking_recv() {
            match command {
                RenderCommand::SetBrush { brush } => {
                    brush_ok = haku.set_brush(&brush).is_ok();
                }
                RenderCommand::Plot { points } => {
                    if brush_ok {
                        if let Ok(value) = haku.eval_brush() {
                            for point in points {
                                // Ignore the result. It's better if we render _something_ rather
                                // than nothing.
                                _ = draw_to_chunks(&haku, value, point, &wall, &chunk_encoder);
                            }
                            haku.reset_vm();
                        }
                    }
                }
            }
        }
    }
}

fn draw_to_chunks(
    haku: &Haku,
    value: Value,
    center: Vec2,
    wall: &Wall,
    chunk_encoder: &ChunkEncoder,
) -> eyre::Result<()> {
    let settings = wall.settings();

    let chunk_size = settings.chunk_size as f32;
    let paint_area = settings.paint_area as f32;

    let left = center.x - paint_area / 2.0;
    let top = center.y - paint_area / 2.0;

    let left_chunk = f32::floor(left / chunk_size) as i32;
    let top_chunk = f32::floor(top / chunk_size) as i32;
    let right_chunk = f32::ceil((left + paint_area) / chunk_size) as i32;
    let bottom_chunk = f32::ceil((top + paint_area) / chunk_size) as i32;
    for chunk_y in top_chunk..bottom_chunk {
        for chunk_x in left_chunk..right_chunk {
            let x = f32::floor(-chunk_x as f32 * chunk_size + center.x);
            let y = f32::floor(-chunk_y as f32 * chunk_size + center.y);
            let chunk_ref = wall.get_or_create_chunk(ChunkPosition::new(chunk_x, chunk_y));
            let mut chunk = chunk_ref.blocking_lock();
            haku.render_value(&mut chunk.pixmap, value, Vec2 { x, y })?;
        }
    }

    for chunk_y in top_chunk..bottom_chunk {
        for chunk_x in left_chunk..right_chunk {
            chunk_encoder.invalidate_blocking(ChunkPosition::new(chunk_x, chunk_y))
        }
    }

    Ok(())
}
