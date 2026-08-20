# Aether Rust SDK

Thin helpers for plugin authors. Plugins never open SurrealDB or craft free-form SurQL for normal CRUD.

## Database

| SDK helper | Host command | Capability |
|---|---|---|
| `Db::get_payload` | `db::get` | `db::query` |
| `Db::find_payload` | `db::find` / `db::query` | `db::query` |
| `Db::create_payload` | `db::create` | `db::mutate` |
| `Db::update_payload` | `db::update` | `db::mutate` |
| `Db::delete_payload` | `db::delete` | `db::mutate` |
| `Db::surql_payload` | `db::surql` | **`db::surql`** (explicit grant) |

Declare models in `plugin.toml` via `access_models` (and/or `[[models]]`). The kernel only allows those names.

```toml
capabilities = ["db::query", "db::mutate"]
# Raw SurQL — omit unless truly required:
# capabilities = ["db::query", "db::mutate", "db::surql"]

access_models = [
  { name = "partner", permissions = ["read", "write"] },
]
```

```rust
use aether_sdk::{commands, Db};
use serde_json::Map;

// Structured find — kernel builds SurQL
let payload = Db::find_payload("partner", Map::from_iter([
    ("active".into(), true.into()),
]), Some(50));
// kernel_command(ctx, commands::DB_FIND, payload).await?;

// Raw SurQL — requires db::surql; bind table via model
let surql = Db::surql_payload(
    "SELECT * FROM type::table($__table) WHERE active = $active",
    Map::from_iter([("active".into(), true.into())]),
    Some("partner"),
);
```
