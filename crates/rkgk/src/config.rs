use serde::{Deserialize, Serialize};

use crate::wall;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub wall_broker: wall::broker::Settings,
    pub haku: crate::haku::Limits,
}
