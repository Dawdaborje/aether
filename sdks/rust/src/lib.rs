//! In-process / future WASM guest SDK surface.
//!
//! Today these helpers document the payload shape plugins should send to
//! [`aether_core::kernel::kernel_command`]. When the Extism host lands,
//! the same JSON shapes are forwarded over the WIT boundary.

use serde_json::{Map, Value as JsonValue};

/// Structured database helpers (no raw SurQL).
pub struct Db;

impl Db {
    pub fn get_payload(model: &str, id: &str) -> JsonValue {
        serde_json::json!({ "model": model, "id": id })
    }

    pub fn find_payload(
        model: &str,
        filter: Map<String, JsonValue>,
        limit: Option<u32>,
    ) -> JsonValue {
        serde_json::json!({
            "model": model,
            "filter": filter,
            "limit": limit,
        })
    }

    pub fn create_payload(model: &str, data: Map<String, JsonValue>) -> JsonValue {
        serde_json::json!({ "model": model, "data": data })
    }

    pub fn update_payload(model: &str, id: &str, data: Map<String, JsonValue>) -> JsonValue {
        serde_json::json!({ "model": model, "id": id, "data": data })
    }

    pub fn delete_payload(model: &str, id: &str) -> JsonValue {
        serde_json::json!({ "model": model, "id": id })
    }

    /// Raw SurQL payload — kernel requires `db::surql`. Prefer structured helpers.
    pub fn surql_payload(
        query: &str,
        vars: Map<String, JsonValue>,
        model: Option<&str>,
    ) -> JsonValue {
        serde_json::json!({
            "query": query,
            "vars": vars,
            "model": model,
        })
    }
}

pub mod commands {
    //! Command name constants matching `capabilities/*.json`.
    pub const DB_GET: &str = "db::get";
    pub const DB_FIND: &str = "db::find";
    pub const DB_QUERY: &str = "db::query";
    pub const DB_CREATE: &str = "db::create";
    pub const DB_UPDATE: &str = "db::update";
    pub const DB_DELETE: &str = "db::delete";
    pub const DB_MUTATE: &str = "db::mutate";
    pub const DB_SURQL: &str = "db::surql";
}
