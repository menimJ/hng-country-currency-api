use axum::{routing::{get, post}, Router};
use tower_http::trace::TraceLayer;

use crate::config::AppState;
use crate::handlers::countries::{
    delete_country, get_country, get_image, health, list_countries, refresh, status,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/countries/refresh", post(refresh))
        .route("/countries", get(list_countries))
        .route("/countries/:name", get(get_country).delete(delete_country))
        .route("/status", get(status))
        .route("/countries/image", get(get_image))
        .route("/healthz", get(health)) // DB health check
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
