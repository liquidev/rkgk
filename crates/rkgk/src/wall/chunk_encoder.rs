use std::sync::Arc;

use indexmap::IndexMap;
use tokio::sync::{mpsc, oneshot};

use super::{ChunkPosition, Wall};

/// Service which encodes chunks to WebP images and caches them in an LRU fashion.
pub struct ChunkEncoder {
    commands_tx: mpsc::Sender<Command>,
}

enum Command {
    GetEncoded {
        chunk: ChunkPosition,
        reply: oneshot::Sender<Option<Arc<[u8]>>>,
    },

    Invalidate {
        chunk: ChunkPosition,
    },
}

impl ChunkEncoder {
    pub fn start(wall: Arc<Wall>) -> Self {
        let (commands_tx, commands_rx) = mpsc::channel(32);

        tokio::spawn(Self::service(wall, commands_rx));

        Self { commands_tx }
    }

    pub async fn encoded(&self, chunk: ChunkPosition) -> Option<Arc<[u8]>> {
        let (tx, rx) = oneshot::channel();
        self.commands_tx
            .send(Command::GetEncoded { chunk, reply: tx })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    pub async fn invalidate(&self, chunk: ChunkPosition) {
        _ = self.commands_tx.send(Command::Invalidate { chunk }).await;
    }

    pub fn invalidate_blocking(&self, chunk: ChunkPosition) {
        _ = self
            .commands_tx
            .blocking_send(Command::Invalidate { chunk });
    }

    async fn encode(wall: &Wall, chunk: ChunkPosition) -> Option<Arc<[u8]>> {
        let pixmap = {
            // Clone out the pixmap to avoid unnecessary chunk mutex contention while the
            // chunk is being encoded.
            let chunk_ref = wall.get_chunk(chunk)?;
            let chunk = chunk_ref.lock().await;
            chunk.pixmap.clone()
        };

        let image = tokio::task::spawn_blocking(move || {
            let webp = webp::Encoder::new(
                pixmap.data(),
                webp::PixelLayout::Rgba,
                pixmap.width(),
                pixmap.height(),
            );
            // NOTE: There's an unnecessary copy here. Wonder if that kills performance much.
            webp.encode_lossless().to_vec()
        })
        .await
        .ok()?;

        Some(Arc::from(image))
    }

    async fn service(wall: Arc<Wall>, mut commands_rx: mpsc::Receiver<Command>) {
        let mut encoded_lru: IndexMap<ChunkPosition, Option<Arc<[u8]>>> = IndexMap::new();

        while let Some(command) = commands_rx.recv().await {
            match command {
                Command::GetEncoded { chunk, reply } => {
                    if let Some(encoded) = encoded_lru.get(&chunk) {
                        _ = reply.send(encoded.clone())
                    } else {
                        let encoded = Self::encode(&wall, chunk).await;
                        // TODO: Make this capacity configurable.
                        // 598 is chosen because under the default configuration, it would
                        // correspond to roughly two 3840x2160 displays.
                        if encoded_lru.len() >= 598 {
                            encoded_lru.shift_remove_index(0);
                        }
                        encoded_lru.insert(chunk, encoded.clone());
                        _ = reply.send(encoded);
                    }
                }

                Command::Invalidate { chunk } => {
                    encoded_lru.shift_remove(&chunk);
                }
            }
        }
    }
}
