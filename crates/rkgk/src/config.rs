use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::wall;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub build: BuildConfig,
    pub wall_broker: wall::broker::Settings,
    pub haku: crate::haku::Limits,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildConfig {
    pub render_templates: Vec<RenderTemplate>,
    pub page_titles: HashMap<PathBuf, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderTemplate {
    pub template: String,
    #[serde(flatten)]
    pub files: RenderTemplateFiles,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RenderTemplateFiles {
    Directory { from_dir: PathBuf, to_dir: PathBuf },
}
