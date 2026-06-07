use crate::api;
use axum::{Router, routing::post};

pub fn routes() -> axum::Router {
    Router::new().route("/login", post(api::login::login))
}
