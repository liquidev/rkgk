use std::{
    ffi::OsStr,
    fs::{copy, create_dir_all, remove_dir_all, write},
};

use copy_dir::copy_dir;
use eyre::Context;
use handlebars::Handlebars;
use import_map::ImportMap;
use include_static::IncludeStatic;
use serde::Serialize;
use static_urls::StaticUrls;
use tracing::{info, instrument};
use walkdir::WalkDir;

pub mod import_map;
mod include_static;
mod static_urls;

use crate::{
    config::{BuildConfig, RenderTemplateFiles},
    Paths,
};

#[instrument(skip(paths, config))]
pub fn build(paths: &Paths<'_>, config: &BuildConfig) -> eyre::Result<()> {
    info!("building static site");

    _ = remove_dir_all(paths.target_dir);
    create_dir_all(paths.target_dir).context("cannot create target directory")?;
    copy_dir("static", paths.target_dir.join("static")).context("cannot copy static directory")?;

    create_dir_all(paths.target_dir.join("static/wasm"))
        .context("cannot create static/wasm directory")?;
    copy(
        paths.target_wasm_dir.join("haku_wasm.wasm"),
        paths.target_dir.join("static/wasm/haku.wasm"),
    )
    .context("cannot copy haku.wasm file")?;

    let import_map = ImportMap::generate("".into(), &config.import_roots);
    write(
        paths.target_dir.join("static/import_map.json"),
        serde_json::to_string(&import_map)?,
    )?;

    let mut handlebars = Handlebars::new();

    handlebars.register_helper(
        "static",
        Box::new(StaticUrls::new(
            paths.target_dir.join("static"),
            "/static".into(),
        )),
    );
    handlebars.register_helper(
        "include_static",
        Box::new(IncludeStatic {
            base_dir: paths.target_dir.join("static"),
        }),
    );

    for entry in WalkDir::new("template") {
        let entry = entry?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if file_name
            .rsplit_once('.')
            .is_some_and(|(left, _)| left.ends_with(".hbs"))
        {
            handlebars.register_template_file(&file_name, path)?;
            info!(file_name, "registered template");
        }
    }

    #[derive(Serialize)]
    struct SingleFileData {}

    #[derive(Serialize)]
    struct DjotData {
        title: String,
        content: String,
    }

    for render_template in &config.render_templates {
        info!(?render_template);
        match &render_template.files {
            RenderTemplateFiles::SingleFile { to_file } => {
                let rendered = handlebars.render(&render_template.template, &SingleFileData {})?;
                std::fs::write(paths.target_dir.join(to_file), rendered)?;
            }

            RenderTemplateFiles::Directory { from_dir, to_dir } => {
                create_dir_all(paths.target_dir.join(to_dir))?;

                for entry in WalkDir::new(from_dir) {
                    let entry = entry?;
                    let inner_path = entry.path().strip_prefix(from_dir)?;

                    if entry.path().extension() == Some(OsStr::new("dj")) {
                        let djot = std::fs::read_to_string(entry.path())?;
                        let events = jotdown::Parser::new(&djot);
                        let content = jotdown::html::render_to_string(events);
                        let title = config
                            .page_titles
                            .get(entry.path())
                            .cloned()
                            .unwrap_or_else(|| entry.path().to_string_lossy().into_owned());
                        let rendered = handlebars
                            .render(&render_template.template, &DjotData { title, content })?;
                        std::fs::write(
                            paths
                                .target_dir
                                .join(to_dir)
                                .join(inner_path.with_extension("html")),
                            rendered,
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}
