use std::{fs::create_dir_all, net::Ipv4Addr, path::Path, sync::Arc};

use api::Api;
use config::Config;
use eyre::Context;
use router::router;
use tokio::{fs, net::TcpListener};
use tracing::info;

mod api;
mod auto_reload;
mod build;
mod config;
mod haku;
mod id;
mod login;
mod router;
mod schema;
mod serialization;
mod wall;

#[cfg(feature = "memory-profiling")]
#[global_allocator]
static GLOBAL_ALLOCATOR: tracy_client::ProfiledAllocator<std::alloc::System> =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

#[derive(Debug, Clone, Copy)]
pub struct Paths<'a> {
    pub target_dir: &'a Path,
    pub target_wasm_dir: &'a Path,
    pub database_dir: &'a Path,
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

    build::build(&paths, &config.build)?;
    let dbs = Arc::new(database(&config, &paths)?);

    let api = Arc::new(Api { config, dbs });
    let app = router(&paths, api);

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
