use std::time::Duration;

use axum::{routing::get, Router};
use tokio::time::sleep;

pub fn router<S>() -> Router<S> {
    Router::new()
        .route("/stall", get(stall))
        .route("/back-up", get(back_up))
        .with_state(())
}

async fn stall() -> String {
    loop {
        // Sleep for a day, I guess. Just to uphold the connection forever without really using any
        // significant resources.
        sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

async fn back_up() -> String {
    "".into()
}
