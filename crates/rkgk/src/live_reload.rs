use std::time::Duration;

use axum::{routing::get, Router};
use tokio::time::sleep;

pub fn router<S>() -> Router<S> {
    let router = Router::new().route("/back-up", get(back_up));

    // The endpoint for immediate reload is only enabled on debug builds.
    // Release builds use the exponential backoff system that detects is the WebSocket is closed.
    #[cfg(debug_assertions)]
    let router = router.route("/stall", get(stall));

    router.with_state(())
}

#[cfg(debug_assertions)]
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
