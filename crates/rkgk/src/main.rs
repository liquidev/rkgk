use std::{
    fs::{copy, create_dir_all, remove_dir_all},
    net::Ipv4Addr,
    path::Path,
    sync::Arc,
};

use api::Api;
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
mod haku;
mod id;
#[cfg(debug_assertions)]
mod live_reload;
mod login;
pub mod schema;
mod serialization;
mod wall;

#[cfg(feature = "memory-profiling")]
#[global_allocator]
static GLOBAL_ALLOCATOR: tracy_client::ProfiledAllocator<std::alloc::System> =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

struct Paths<'a> {
    target_dir: &'a Path,
    target_wasm_dir: &'a Path,
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
        paths.target_wasm_dir.join("haku_wasm.wasm"),
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

    create_dir_all(paths.database_dir.join("wall"))?;
    let wall_broker =
        wall::Broker::new(paths.database_dir.join("wall"), config.wall_broker.clone());

    Ok(Databases { login, wall_broker })
}

async fn fallible_main() -> eyre::Result<()> {
    let target_wasm_dir =
        std::env::var("RKGK_WASM_PATH").unwrap_or("target/wasm32-unknown-unknown/wasm-dev".into());
    let paths = Paths {
        target_wasm_dir: Path::new(&target_wasm_dir),
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

    let api = Arc::new(Api { config, dbs });
    let app = Router::new()
        .route_service(
            "/",
            ServeFile::new(paths.target_dir.join("static/index.html")),
        )
        .nest_service("/static", ServeDir::new(paths.target_dir.join("static")))
        .nest("/api", api::router(api));

    #[cfg(debug_assertions)]
    let app = app.nest("/dev/live-reload", live_reload::router());

    let port: u16 = std::env::var("RKGK_PORT")
        .unwrap_or("8080".into())
        .parse()
        .context("failed to parse RKGK_PORT")?;

    let listener = TcpListener::bind((Ipv4Addr::from([0u8, 0, 0, 0]), port))
        .await
        .expect("cannot bind to port");
    info!("listening on port {port}");
    axum::serve(listener, app).await.expect("cannot serve app");

    Ok(())
}

#[tokio::main]
async fn main() {
    #[cfg(feature = "memory-profiling")]
    let _client = tracy_client::Client::start();

    color_eyre::install().unwrap();
    tracing_subscriber::fmt().init();

    match fallible_main().await {
        Ok(_) => (),
        Err(error) => println!("{error:?}"),
    }
}
