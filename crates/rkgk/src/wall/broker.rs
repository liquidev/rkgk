use std::sync::Arc;

use dashmap::DashMap;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::Mutex;
use tracing::info;

use super::{chunk_encoder::ChunkEncoder, Settings, Wall, WallId};

/// The broker is the main way to access wall data.
///
/// It handles dynamically loading and unloading walls as they're needed.
/// It also handles database threads for each wall.
pub struct Broker {
    wall_settings: Settings,
    open_walls: DashMap<WallId, OpenWall>,
    rng: Mutex<ChaCha20Rng>,
}

#[derive(Clone)]
pub struct OpenWall {
    pub wall: Arc<Wall>,
    pub chunk_encoder: Arc<ChunkEncoder>,
}

impl Broker {
    pub fn new(wall_settings: Settings) -> Self {
        info!(?wall_settings, "Broker::new");
        Self {
            wall_settings,
            open_walls: DashMap::new(),
            rng: Mutex::new(ChaCha20Rng::from_entropy()),
        }
    }

    pub async fn generate_id(&self) -> WallId {
        // TODO: Will lock contention be an issue with generating wall IDs?
        // We only have one of these RNGs per rkgk instance.
        let mut rng = self.rng.lock().await;
        WallId::new(&mut *rng)
    }

    pub fn open(&self, wall_id: WallId) -> OpenWall {
        let wall = Arc::new(Wall::new(self.wall_settings));
        self.open_walls
            .entry(wall_id)
            .or_insert_with(|| OpenWall {
                chunk_encoder: Arc::new(ChunkEncoder::start(Arc::clone(&wall))),
                wall,
            })
            .clone()
    }
}
