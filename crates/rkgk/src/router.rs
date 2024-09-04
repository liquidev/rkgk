use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        HeaderValue,
    },
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::{
    api::{self, Api},
    auto_reload, Paths,
};

struct Server {
    target_dir: PathBuf,

    index_html: String,
    four_oh_four_html: String,
}

pub fn router<S>(paths: &Paths, api: Arc<Api>) -> Router<S> {
    Router::new()
        .route("/", get(index))
        .route("/static/*path", get(static_file))
        .route("/docs/*path", get(docs))
        .nest("/api", api::router(api))
        .nest("/auto-reload", auto_reload::router())
        .fallback(get(four_oh_four))
        .with_state(Arc::new(Server {
            target_dir: paths.target_dir.to_path_buf(),

            index_html: std::fs::read_to_string(paths.target_dir.join("static/index.html"))
                .expect("index.html does not exist"),
            four_oh_four_html: std::fs::read_to_string(paths.target_dir.join("static/404.html"))
                .expect("404.html does not exist"),
        }))
}

async fn index(State(state): State<Arc<Server>>) -> Html<String> {
    Html(state.index_html.clone())
}

async fn four_oh_four(State(state): State<Arc<Server>>) -> Html<String> {
    Html(state.four_oh_four_html.clone())
}

#[derive(Deserialize)]
struct StaticFileQuery {
    cache: Option<String>,
}

async fn static_file(
    Path(path): Path<String>,
    Query(query): Query<StaticFileQuery>,
    State(state): State<Arc<Server>>,
) -> Response {
    if let Ok(file) = tokio::fs::read(state.target_dir.join("static").join(&path)).await {
        let mut response = file.into_response();

        if let Some(content_type) = mime_guess::from_path(&path).first_raw() {
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        } else {
            response.headers_mut().remove(CONTENT_TYPE);
        }

        if query.cache.is_some() {
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
        }

        response
    } else {
        four_oh_four(State(state)).await.into_response()
    }
}

async fn docs(Path(mut path): Path<String>, state: State<Arc<Server>>) -> Html<String> {
    if !path.ends_with(".html") {
        path.push_str(".html")
    }

    if let Ok(file) = tokio::fs::read_to_string(state.target_dir.join("docs").join(&path)).await {
        Html(file)
    } else {
        four_oh_four(state).await
    }
}
