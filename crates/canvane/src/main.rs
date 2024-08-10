use std::{
    fs::{copy, create_dir_all, remove_dir_all},
    path::Path,
};

use axum::Router;
use copy_dir::copy_dir;
use eyre::Context;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{info, info_span};
use tracing_subscriber::fmt::format::FmtSpan;

#[cfg(debug_assertions)]
mod live_reload;

struct Paths<'a> {
    target_dir: &'a Path,
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

#[tokio::main]
async fn main() {
    color_eyre::install().unwrap();
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::ACTIVE)
        .init();

    let paths = Paths {
        target_dir: Path::new("target/site"),
    };

    match build(&paths) {
        Ok(()) => (),
        Err(error) => eprintln!("{error:?}"),
    }

    let app = Router::new()
        .route_service(
            "/",
            ServeFile::new(paths.target_dir.join("static/index.html")),
        )
        .nest_service("/static", ServeDir::new(paths.target_dir.join("static")));

    #[cfg(debug_assertions)]
    let app = app.nest("/dev/live-reload", live_reload::router());

    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("cannot bind to port");
    info!("listening on port 8080");
    axum::serve(listener, app).await.expect("cannot serve app");
}
