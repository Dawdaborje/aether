mod pages;
mod theme;

use axum::Router;

pub fn router() -> Router {
    Router::new()
        .merge(theme::router())
        .merge(pages::router())
}
