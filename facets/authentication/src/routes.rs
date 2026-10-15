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
        .route("/magic-link/consume", post(api::consume_magic_link))
        .route("/me", get(api::me).patch(api::patch_me))
        .route(
            "/me/avatar",
            get(api::get_avatar)
                .post(api::upload_avatar)
                .delete(api::delete_avatar),
        )
        .route("/orgs", get(api::list_orgs))
        .route("/org", post(api::switch_org).delete(api::leave_org))
        .route("/oauth/{provider}/start", get(api::oauth_start))
        .route("/oauth/{provider}/callback", get(api::oauth_callback))
}
