use serde::{Deserialize, Serialize};

use crate::wall;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub wall: wall::Settings,
}
