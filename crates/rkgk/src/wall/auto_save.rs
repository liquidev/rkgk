use std::{collections::HashSet, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc,
    time::{interval, MissedTickBehavior},
};
use tracing::instrument;

use super::{chunk_images::ChunkImages, ChunkPosition};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub interval_seconds: u64,
}

pub struct AutoSave {
    requests_tx: mpsc::Sender<Vec<ChunkPosition>>,
}

impl AutoSave {
    pub fn new(chunk_images: Arc<ChunkImages>, settings: Settings) -> Self {
        let (requests_tx, requests_rx) = mpsc::channel(8);

        tokio::spawn(
            AutoSaveLoop {
                chunk_images,
                settings,
                requests_rx,
                unsaved_chunks: HashSet::new(),
            }
            .enter(),
        );

        Self { requests_tx }
    }

    pub async fn request(&self, chunks: Vec<ChunkPosition>) {
        _ = self.requests_tx.send(chunks).await;
    }
}

struct AutoSaveLoop {
    chunk_images: Arc<ChunkImages>,
    settings: Settings,

    requests_rx: mpsc::Receiver<Vec<ChunkPosition>>,
    unsaved_chunks: HashSet<ChunkPosition>,
}

impl AutoSaveLoop {
    async fn enter(mut self) {
        let mut save_interval = interval(Duration::from_secs(self.settings.interval_seconds));
        save_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                request = self.requests_rx.recv() => {
                    if let Some(positions) = request {
                        for position in positions {
                            self.unsaved_chunks.insert(position);
                        }
                    } else {
                        break;
                    }
                }

                _ = save_interval.tick() => self.save_chunks().await,

                else => break,
            }
        }
    }

    #[instrument(skip(self), fields(num_chunks = self.unsaved_chunks.len()))]
    async fn save_chunks(&mut self) {
        // NOTE: We don't care about actually using the images here -
        // the ChunkImages service writes them to the database by itself, and that's all our
        // request is for.
        _ = self
            .chunk_images
            .encoded(self.unsaved_chunks.drain().collect())
            .await;
    }
}
