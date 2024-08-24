use std::{convert::identity, path::PathBuf, sync::Arc};

use eyre::Context;
use rusqlite::Connection;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, instrument};

use super::{ChunkPosition, WallId};

pub struct Settings {
    pub path: PathBuf,
    pub wall_id: WallId,
    pub default_wall_settings: super::Settings,
}

pub struct Database {
    wall_settings: super::Settings,
    command_tx: mpsc::Sender<Command>,
}

#[derive(Debug, Clone)]
pub struct ChunkDataPair {
    pub position: ChunkPosition,
    pub data: Arc<[u8]>,
}

enum Command {
    Write {
        chunks: Vec<ChunkDataPair>,
        reply: oneshot::Sender<eyre::Result<()>>,
    },

    Read {
        chunks: Vec<ChunkPosition>,
        reply: oneshot::Sender<Vec<ChunkDataPair>>,
    },

    GetAllChunks {
        reply: oneshot::Sender<eyre::Result<Vec<ChunkPosition>>>,
    },
}

impl Database {
    pub fn wall_settings(&self) -> &super::Settings {
        &self.wall_settings
    }

    #[instrument(skip(self), err(Debug))]
    pub async fn write_chunks(&self, chunks: Vec<ChunkDataPair>) -> eyre::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::Write { chunks, reply: tx })
            .await
            .context("database is offline")?;
        rx.await.context("database returned an error")?
    }

    pub async fn read_chunks(
        &self,
        chunks: Vec<ChunkPosition>,
    ) -> eyre::Result<Vec<ChunkDataPair>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::Read { chunks, reply: tx })
            .await
            .context("database is offline")?;
        rx.await.context("database did not return anything")
    }

    pub async fn get_all_chunks(&self) -> eyre::Result<Vec<ChunkPosition>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::GetAllChunks { reply: tx })
            .await
            .context("database is offline")?;
        rx.await.context("database did not return anything")?
    }
}

#[instrument(name = "wall::database::start", skip(settings), fields(wall_id = %settings.wall_id))]
pub fn start(settings: Settings) -> eyre::Result<Database> {
    let db = Connection::open(settings.path).context("cannot open wall database")?;

    let major: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();
    let minor: u32 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap();
    let patch: u32 = env!("CARGO_PKG_VERSION_PATCH").parse().unwrap();
    let version = major * 1_000_000 + minor * 1_000 + patch;

    info!("initial setup");

    db.execute_batch(
        r#"
            PRAGMA application_id = 0x726B6757; -- rkgW

            CREATE TABLE IF NOT EXISTS
            t_file_info (
                id      INTEGER PRIMARY KEY CHECK (id = 1),
                version INTEGER NOT NULL,
                wall_id BLOB NOT NULL,
                mtime   INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS
            t_wall_settings (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                max_chunks   INTEGER NOT NULL,
                max_sessions INTEGER NOT NULL,
                paint_area   INTEGER NOT NULL,
                chunk_size   INTEGER NOT NULL
            );
        
            CREATE TABLE IF NOT EXISTS
            t_wall_info (
                id         INTEGER PRIMARY KEY CHECK (id = 1),
                created_by BLOB NOT NULL,
                title      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS
            t_chunks_v0 (
                chunk_index INTEGER PRIMARY KEY,
                chunk_x     INTEGER NOT NULL,
                chunk_y     INTEGER NOT NULL,
                mtime       INTEGER NOT NULL DEFAULT (unixepoch()),
                webp_data   BLOB NOT NULL,

                UNIQUE(chunk_x, chunk_y) ON CONFLICT REPLACE
            );

            CREATE INDEX IF NOT EXISTS
            t_chunks_v0_index_xy ON t_chunks_v0
            (chunk_x, chunk_y);
        "#,
    )?;

    info!("set file version and wall ID");

    db.execute(
        r#"
            INSERT OR IGNORE
            INTO t_file_info
            (version, wall_id, mtime)
            VALUES (?, ?, unixepoch());
        "#,
        (version, settings.wall_id.0),
    )?;

    info!("set wall mtime");

    db.execute(
        r#"
            UPDATE t_file_info
            SET mtime = unixepoch();
        "#,
        (),
    )?;

    info!("initialize/get wall settings");

    db.execute(
        r#"
            INSERT OR IGNORE
            INTO t_wall_settings
            (max_chunks, max_sessions, paint_area, chunk_size)
            VALUES (?, ?, ?, ?);
        "#,
        (
            settings.default_wall_settings.max_chunks,
            settings.default_wall_settings.max_sessions,
            settings.default_wall_settings.paint_area,
            settings.default_wall_settings.chunk_size,
        ),
    )?;

    let wall_settings = db.query_row(
        r#"
            SELECT
            max_chunks, max_sessions, paint_area, chunk_size
            FROM t_wall_settings;
        "#,
        (),
        |row| {
            Ok(super::Settings {
                max_chunks: row.get(0)?,
                max_sessions: row.get(1)?,
                paint_area: row.get(2)?,
                chunk_size: row.get(3)?,
            })
        },
    )?;

    let (command_tx, mut command_rx) = mpsc::channel(8);

    std::thread::Builder::new()
        .name(format!("database thread {}", settings.wall_id))
        .spawn(move || {
            let mut s_write_chunk = db
                .prepare(
                    r#"
                        INSERT
                        INTO t_chunks_v0
                        (chunk_x, chunk_y, webp_data, mtime)
                        VALUES (?, ?, ?, unixepoch());
                    "#,
                )
                .unwrap();

            let mut s_read_chunk = db
                .prepare(
                    r#"
                        SELECT webp_data
                        FROM t_chunks_v0
                        WHERE chunk_x = ? AND chunk_y = ?;
                    "#,
                )
                .unwrap();

            let mut s_get_all_chunks = db
                .prepare(
                    r#"
                        SELECT chunk_x, chunk_y
                        FROM t_chunks_v0;
                    "#,
                )
                .unwrap();

            while let Some(command) = command_rx.blocking_recv() {
                match command {
                    Command::Write { chunks, reply } => {
                        let mut result = Ok(());
                        for ChunkDataPair { position, data } in chunks {
                            if let Err(error) =
                                s_write_chunk.execute((position.x, position.y, &data[..]))
                            {
                                result = Err(error).with_context(|| {
                                    format!("failed to update chunk at {position:?}")
                                });
                            }
                        }
                        _ = reply.send(result.context(
                            "failed to update one or more chunks; see context for last error",
                        ));
                    }

                    Command::Read { chunks, reply } => {
                        let result = chunks
                            .into_iter()
                            .flat_map(|position| {
                                s_read_chunk
                                    .query_row((position.x, position.y), |row| {
                                        Ok(ChunkDataPair {
                                            position,
                                            data: Arc::from(row.get::<_, Vec<u8>>(0)?),
                                        })
                                    })
                                    .inspect_err(|err| {
                                        if err != &rusqlite::Error::QueryReturnedNoRows {
                                            error!(?err, ?position, "while reading chunk");
                                        }
                                    })
                                    .ok()
                            })
                            .collect();
                        _ = reply.send(result);
                    }

                    Command::GetAllChunks { reply } => {
                        _ = reply.send(
                            s_get_all_chunks
                                .query_map((), |row| {
                                    Ok(ChunkPosition::new(row.get(0)?, row.get(1)?))
                                })
                                .map(|chunks| chunks.collect::<Result<_, _>>())
                                .and_then(identity)
                                .context("failed to query all chunks"),
                        )
                    }
                }
            }
        })
        .context("cannot spawn thread")?;

    Ok(Database {
        command_tx,
        wall_settings,
    })
}
