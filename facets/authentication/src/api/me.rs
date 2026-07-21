use axum::Json;
use serde_json::{Value as JsonValue, json};

use crate::authz::AuthSession;

pub async fn me(AuthSession(session): AuthSession) -> Json<JsonValue> {
    Json(json!({
        "user": {
            "id": format!("{:?}", session.user.id),
            "username": session.user.username,
            "email": session.user.email,
            "display_name": session.user.display_name,
            "is_super_user": session.user.is_super_user,
        },
        "session": {
            "provider": session.provider,
            "org_database_id": session.org_database_id,
        }
    }))
}
