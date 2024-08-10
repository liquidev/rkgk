use std::{
    fs::{copy, create_dir_all, remove_dir_all},
    path::Path,
    sync::Arc,
};

use axum::Router;
use config::Config;
use copy_dir::copy_dir;
use eyre::Context;
use tokio::{fs, net::TcpListener};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{info, info_span};
use tracing_subscriber::fmt::format::FmtSpan;

mod api;
mod binary;
mod config;
mod id;
#[cfg(debug_assertions)]
mod live_reload;
mod login;
pub mod schema;
mod serialization;
mod wall;

struct Paths<'a> {
    target_dir: &'a Path,
    database_dir: &'a Path,
}

fn build(paths: &Paths<'_>) -> eyre::Result<()> {
    let _span = info_span!("build").entered();

    _ = remove_dir_all(paths.target_dir);
    create_dir_all(paths.target_dir).context("cannot create target directory")?;
    copy_dir("static", paths.target_dir.join("static")).context("cannot copy static directory")?;

    create_dir_all(paths.target_dir.join("static/wasm"))
        .context("cannot create static/wasm directory")?;
    copy(
        "target/wasm32-unknown-unknown/wasm-dev/haku_wasm.wasm",
        paths.target_dir.join("static/wasm/haku.wasm"),
    )
    .context("cannot copy haku.wasm file")?;

    Ok(())
}

pub struct Databases {
    pub login: login::Database,
    pub wall_broker: wall::Broker,
}

fn database(config: &Config, paths: &Paths<'_>) -> eyre::Result<Databases> {
    create_dir_all(paths.database_dir).context("cannot create directory for databases")?;

    let login = login::database::start(&login::database::Settings {
        path: paths.database_dir.join("login.db"),
    })
    .context("cannot start up login database")?;

    let wall_broker = wall::Broker::new(config.wall);

    Ok(Databases { login, wall_broker })
}

async fn fallible_main() -> eyre::Result<()> {
    let paths = Paths {
        target_dir: Path::new("target/site"),
        database_dir: Path::new("database"),
    };

    let config: Config = toml::from_str(
        &fs::read_to_string("rkgk.toml")
            .await
            .context("cannot read config file")?,
    )
    .context("cannot deserialize config file")?;

    build(&paths)?;
    let dbs = Arc::new(database(&config, &paths)?);

    let app = Router::new()
        .route_service(
            "/",
            ServeFile::new(paths.target_dir.join("static/index.html")),
        )
        .nest_service("/static", ServeDir::new(paths.target_dir.join("static")))
        .nest("/api", api::router(dbs.clone()));

    #[cfg(debug_assertions)]
    let app = app.nest("/dev/live-reload", live_reload::router());

    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("cannot bind to port");
    info!("listening on port 8080");
    axum::serve(listener, app).await.expect("cannot serve app");

    Ok(())
}

#[tokio::main]
async fn main() {
    color_eyre::install().unwrap();
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::ACTIVE)
        .init();

    match fallible_main().await {
        Ok(_) => (),
        Err(error) => println!("{error:?}"),
    }
}
