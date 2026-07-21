mod pages;
mod theme;

use axum::Router;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .merge(theme::router())
        .merge(pages::router())
}
