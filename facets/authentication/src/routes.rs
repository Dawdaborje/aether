use aether_core::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

use crate::api;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/methods", get(api::methods))
        .route("/login", post(api::login))
        .route("/logout", post(api::logout))
        .route("/me", get(api::me))
        .route("/oauth/{provider}/start", get(api::oauth_start))
        .route("/oauth/{provider}/callback", get(api::oauth_callback))
}
