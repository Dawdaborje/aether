use aether_core::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

use crate::{api, throttle};

pub fn routes(state: AppState) -> Router<AppState> {
    // Routes a stranger can hammer without a credential: each address gets a budget per minute.
    let sign_in = Router::new()
        .route("/login", post(api::login))
        .route("/magic-link/consume", post(api::consume_magic_link))
        .route("/oauth/{provider}/start", get(api::oauth_start))
        .route("/oauth/{provider}/callback", get(api::oauth_callback))
        .route_layer(axum::middleware::from_fn_with_state(state, throttle::limit_sign_in));
    Router::new()
        .merge(sign_in)
        .route("/methods", get(api::methods))
        .route("/logout", post(api::logout))
        .route("/me", get(api::me).patch(api::patch_me))
        .route(
            "/me/avatar",
            get(api::get_avatar)
                .post(api::upload_avatar)
                .delete(api::delete_avatar),
        )
        .route("/users/{key}/avatar", get(api::get_member_avatar))
        .route("/orgs", get(api::list_orgs))
}
