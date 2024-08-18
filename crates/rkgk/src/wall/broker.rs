use std::{path::PathBuf, sync::Arc};

use dashmap::DashMap;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, instrument};

use super::{
    auto_save::{self, AutoSave},
    chunk_images::ChunkImages,
    database, Database, Wall, WallId,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub default_wall_settings: super::Settings,
    pub auto_save: auto_save::Settings,
}

/// The broker is the main way to access wall data.
///
/// It handles dynamically loading and unloading walls as they're needed.
/// It also handles database threads for each wall.
pub struct Broker {
    databases_dir: PathBuf,
    settings: Settings,
    open_walls: DashMap<WallId, OpenWall>,
    rng: Mutex<ChaCha20Rng>,
}

#[derive(Clone)]
pub struct OpenWall {
    pub wall: Arc<Wall>,
    pub chunk_images: Arc<ChunkImages>,
    pub db: Arc<Database>,
    pub auto_save: Arc<AutoSave>,
}

impl Broker {
    pub fn new(databases_dir: PathBuf, settings: Settings) -> Self {
        info!(?settings, "Broker::new");
        Self {
            databases_dir,
            settings,
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

    #[instrument(skip(self), fields(%wall_id))]
    pub async fn open(&self, wall_id: WallId) -> eyre::Result<OpenWall> {
        let open_wall = self.open_walls.entry(wall_id);

        match open_wall {
            dashmap::Entry::Vacant(entry) => {
                let db = Arc::new(database::start(database::Settings {
                    path: self.databases_dir.join(format!("{wall_id}.db")),
                    wall_id,
                    default_wall_settings: self.settings.default_wall_settings,
                })?);
                let wall = Arc::new(Wall::new(*db.wall_settings()));
                let chunk_images = Arc::new(ChunkImages::new(Arc::clone(&wall), Arc::clone(&db)));
                let auto_save = Arc::new(AutoSave::new(
                    Arc::clone(&chunk_images),
                    self.settings.auto_save.clone(),
                ));
                let open_wall = OpenWall {
                    wall,
                    chunk_images,
                    db,
                    auto_save,
                };

                entry.insert(open_wall.clone());

                Ok(open_wall)
            }
            dashmap::Entry::Occupied(entry) => Ok(entry.get().clone()),
        }
    }
}
