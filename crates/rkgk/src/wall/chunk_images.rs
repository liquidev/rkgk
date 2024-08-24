use std::sync::Arc;

use dashmap::DashSet;
use eyre::Context;
use haku::render::tiny_skia::{IntSize, Pixmap};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, instrument};

use super::{database::ChunkDataPair, ChunkPosition, Database, Wall};

/// Chunk image encoding, caching, and storage service.
pub struct ChunkImages {
    wall: Arc<Wall>,
    async_loop: Arc<ChunkImageLoop>,
    commands_tx: mpsc::Sender<Command>,
}

enum Command {
    Encode {
        chunks: Vec<ChunkPosition>,
        reply: oneshot::Sender<Vec<ChunkDataPair>>,
    },

    Load {
        chunks: Vec<ChunkPosition>,
        reply: oneshot::Sender<eyre::Result<()>>,
    },
}

impl ChunkImages {
    pub fn new(wall: Arc<Wall>, db: Arc<Database>) -> Self {
        let (commands_tx, commands_rx) = mpsc::channel(32);

        let async_loop = Arc::new(ChunkImageLoop {
            wall: Arc::clone(&wall),
            db,
            chunks_in_db: DashSet::new(),
        });
        tokio::spawn(Arc::clone(&async_loop).enter(commands_rx));

        Self {
            wall,
            async_loop,
            commands_tx,
        }
    }

    pub async fn encoded(&self, chunks: Vec<ChunkPosition>) -> Vec<ChunkDataPair> {
        let (tx, rx) = oneshot::channel();
        _ = self
            .commands_tx
            .send(Command::Encode { chunks, reply: tx })
            .await
            .ok();
        rx.await.ok().unwrap_or_default()
    }

    pub async fn load(&self, chunks: Vec<ChunkPosition>) -> eyre::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.commands_tx
            .send(Command::Load { chunks, reply: tx })
            .await
            .context("database is offline")?;
        rx.await.context("failed to load chunks")?
    }

    pub fn chunk_exists(&self, position: ChunkPosition) -> bool {
        self.wall.has_chunk(position) || self.async_loop.chunks_in_db.contains(&position)
    }
}

struct ChunkImageLoop {
    wall: Arc<Wall>,
    db: Arc<Database>,
    chunks_in_db: DashSet<ChunkPosition>,
}

impl ChunkImageLoop {
    #[instrument(skip(self, reply))]
    async fn encode(
        self: Arc<Self>,
        mut chunks: Vec<ChunkPosition>,
        reply: oneshot::Sender<Vec<ChunkDataPair>>,
    ) {
        // Any chunks that are _not_ loaded, we will send back directly from the database.
        let mut unloaded = vec![];
        chunks.retain(|&position| {
            let is_loaded = self.wall.has_chunk(position);
            unloaded.push(position);
            is_loaded
        });

        // Any chunks that _are_ loaded, we will encode into WebP.
        let this = Arc::clone(&self);
        let loaded = tokio::task::spawn_blocking(move || {
            chunks
                .into_par_iter()
                .flat_map(|position| -> Option<_> {
                    let pixmap = {
                        // Clone out the pixmap to avoid unnecessary chunk mutex contention while the
                        // chunk is being encoded.
                        let chunk_ref = this.wall.get_chunk(position)?;
                        let chunk = chunk_ref.blocking_lock();
                        chunk.pixmap.clone()
                    };

                    let webp = webp::Encoder::new(
                        pixmap.data(),
                        webp::PixelLayout::Rgba,
                        pixmap.width(),
                        pixmap.height(),
                    );
                    // NOTE: There's an unnecessary copy here. Wonder if that kills performance much.
                    Some(ChunkDataPair {
                        position,
                        data: Arc::from(webp.encode_lossless().to_vec()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap();

        // We'll also write the loaded chunks back to the database while we have the images.
        // No point wasting the encoding time.
        if !loaded.is_empty() {
            info!(num = loaded.len(), "writing loaded chunks to database");
        }
        _ = self.db.write_chunks(loaded.clone()).await;
        for &ChunkDataPair { position, .. } in &loaded {
            self.chunks_in_db.insert(position);
        }

        let mut all = loaded;
        match self.db.read_chunks(unloaded).await {
            Ok(mut chunks) => all.append(&mut chunks),
            Err(err) => error!(?err, "read_chunks failed to read unloaded chunks"),
        }

        _ = reply.send(all);
    }

    async fn load_inner(self: Arc<Self>, mut chunks: Vec<ChunkPosition>) -> eyre::Result<()> {
        // Skip already loaded chunks.
        chunks.retain(|&position| !self.wall.has_chunk(position));
        if chunks.is_empty() {
            return Ok(());
        }

        info!(?chunks, "to load");

        let chunks = self.db.read_chunks(chunks.clone()).await?;

        let chunks2 = chunks.clone();
        let decoded = tokio::task::spawn_blocking(move || {
            chunks2
                .par_iter()
                .flat_map(|ChunkDataPair { position, data }| {
                    webp::Decoder::new(data)
                        .decode()
                        .and_then(|image| {
                            info!(
                                ?position,
                                width = image.width(),
                                height = image.height(),
                                data_len = image.len(),
                                "decoded"
                            );
                            let image = image.to_image().into_rgba8();
                            let size = IntSize::from_wh(image.width(), image.height())?;
                            Pixmap::from_vec(image.to_vec(), size)
                        })
                        .map(|pixmap| (*position, pixmap))
                })
                .collect::<Vec<(ChunkPosition, Pixmap)>>()
        })
        .await
        .context("failed to decode chunks from the database")?;

        // I don't know yet if locking all the chunks is a good idea at this point.
        // I can imagine contended chunks having some trouble loading.
        let chunk_arcs: Vec<_> = decoded
            .iter()
            .map(|(position, _)| self.wall.get_or_create_chunk(*position))
            .collect();
        let mut chunk_refs = Vec::with_capacity(chunk_arcs.len());
        for arc in &chunk_arcs {
            chunk_refs.push(arc.lock().await);
        }

        info!(num = ?chunk_refs.len(), "replacing chunks' pixmaps");
        for ((_, pixmap), mut chunk) in decoded.into_iter().zip(chunk_refs) {
            chunk.pixmap = pixmap;
        }

        Ok(())
    }

    #[instrument(skip(self, reply))]
    async fn load(
        self: Arc<Self>,
        chunks: Vec<ChunkPosition>,
        reply: oneshot::Sender<eyre::Result<()>>,
    ) {
        _ = reply.send(self.load_inner(chunks).await);
    }

    async fn enter(self: Arc<Self>, mut commands_rx: mpsc::Receiver<Command>) {
        let all_chunks = self
            .db
            .get_all_chunks()
            .await
            .expect("could not list chunks in the database");
        for position in all_chunks {
            self.chunks_in_db.insert(position);
        }

        while let Some(command) = commands_rx.recv().await {
            match command {
                Command::Encode { chunks, reply } => {
                    tokio::spawn(Arc::clone(&self).encode(chunks, reply));
                }

                Command::Load { chunks, reply } => {
                    tokio::spawn(Arc::clone(&self).load(chunks, reply));
                }
            }
        }
    }
}
