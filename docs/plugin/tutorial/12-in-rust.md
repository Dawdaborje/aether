---
title: "Appendix: the same plugin in Rust"
description: "The finished Tasks plugin written in Rust and compiled to WebAssembly: what stays the same and what changes."
section: plugin
status: stable
order: 24
---
# Appendix: the same plugin in Rust

Rhai is the shortest way to write a plugin, and the plugin from chapter 10 never needs anything else. This appendix writes
the same plugin in Rust, to show what changes when a plugin is compiled to WebAssembly. Read it when you expect heavy
computation, a library, or a code base too big for one script. The same steps work for Go, TypeScript and the other
languages; only the SDK differs (see [SDK](../../sdk.md)).

The finished plugin is in the `rust/tasks` folder, next to the finished Rhai one in `tasks`. It has the same name, models, rules, pages, roles,
translations and commands, so you install one **or** the other, not both.

## What stays the same

Everything that is not code: `models/`, `rules/`, `pages/`, `i18n/`, `docs/`, the `[app]` table, the roles, the events,
the schedule and the commands in `plugin.toml`. The kernel runs the same checks whatever the language, so a rule that
refuses a delete in Rhai refuses it here too. In the copy for this appendix, only one line of `plugin.toml` changed: a
script is replaced by a compiled file.

<!-- file: rust/tasks/plugin.toml#11-11 -->
```toml
wasm_file = "out/plugin.wasm"
```

Compare it with `script = "main.rhai"` in chapter 1.

## What you need

- Rust, with the WebAssembly target: `rustup target add wasm32-unknown-unknown`.
- The Aether Rust SDK, the `sdks/rust` folder of the SDK repository. It is used by path, so point to wherever you keep it.
- `make`, for the two commands that build the file.

`aether --gen plugin --plugin-path plugins/tasks --plugin-language rust` writes a starter with the files below. Start from
it, or copy `rust/tasks` and change what you need.

## The files

<!-- file: rust/tasks/Cargo.toml -->
```toml
[package]
name = "tasks"
version = "1.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
aether-sdk = { path = "../../../../../sdks/rust" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
```

A plugin is a library of the `cdylib` kind. The SDK handles the plumbing between your functions and the kernel, so you do
not deal with memory or JSON envelopes. `serde` turns the input of a call into your types.

<!-- file: rust/tasks/Makefile -->
```make
.DEFAULT_GOAL := build

build:
	cargo build --release --target wasm32-unknown-unknown
	mkdir -p out
	cp target/wasm32-unknown-unknown/release/tasks.wasm out/plugin.wasm

clean:
	cargo clean
	rm -rf out

.PHONY: build clean
```

`make` builds the file and puts it where `plugin.toml` says: `out/plugin.wasm`. Run it after every change to the code. With
`--serve --watch` running, the server reloads the plugin when `out/plugin.wasm` changes, so `make` is the whole loop.

## The code

Each function you export to the kernel is written inside `handler!`. The function name is the name a page, a command or
another plugin calls, exactly like a Rhai function.

<!-- file: rust/tasks/src/lib.rs#1-19 -->
```rust
// Tasks, in Rust: the same plugin as the Rhai one, compiled to WebAssembly.
use aether_sdk::db::Filter;
use aether_sdk::prelude::*;
use serde_json::{json, Map, Value};

const MODEL: &str = "task";

#[derive(Deserialize)]
struct NewTask {
    title: Option<String>,
    details: Option<String>,
    due: Option<String>,
    assignee: Option<String>,
}

#[derive(Deserialize)]
struct ById {
    id: String,
}
```

The model's name is a constant, and the input of `add_task` is a type: a missing `title` is simply `None`.

<!-- file: rust/tasks/src/lib.rs#21-26 -->
```rust
handler! {
    fn hello(_input: Empty) -> Value {
        let who = context::current()?;
        Ok(json!({ "message": "Hello from tasks", "actor": who.actor }))
    }
}
```

`context::current()` is `context::get()` of Rhai. `Empty` means the call takes no input.

<!-- file: rust/tasks/src/lib.rs#28-41 -->
```rust
handler! {
    fn add_task(input: NewTask) -> Value {
        let title = input.title.unwrap_or_default();
        if title.is_empty() {
            return Err(Error::msg(i18n::t("task.need_title")?));
        }
        db::create(MODEL, &json!({
            "title": title,
            "details": input.details,
            "due": input.due,
            "assignee": input.assignee,
        }))
    }
}
```

Two differences from the Rhai version:

- `Err(Error::msg(...))` is `fail(...)`. The text is shown to the person, as a 422, and anything else (a database failure,
  a missing capability) stays internal.
- The message comes from `i18n::t`, as in chapter 9, so it follows the language of the request. The SDK function is
  `i18n::t(key)?` (it can fail like any call to the kernel); `i18n::t_with(key, &params)` fills `{name}` places and
  `i18n::t_in(key, "fr", &params)` names the language.

<!-- file: rust/tasks/src/lib.rs#43-47 -->
```rust
handler! {
    fn list_tasks(_input: Empty) -> Vec<Value> {
        db::find::<Value>(MODEL).order_by("due").limit(200).all()
    }
}
```

<!-- file: rust/tasks/src/lib.rs#49-53 -->
```rust
handler! {
    fn get_task(input: ById) -> Option<Value> {
        db::get(MODEL, &input.id)
    }
}
```

<!-- file: rust/tasks/src/lib.rs#55-70 -->
```rust
handler! {
    fn update_task(input: Map<String, Value>) -> Option<Value> {
        let mut changes = input;
        let id = match changes.remove("id") {
            Some(Value::String(id)) => id,
            _ => return Err(Error::msg("An id is needed.")),
        };
        let task: Option<Value> = db::update(MODEL, &id, &changes)?;
        if let Some(task) = &task {
            if task["status"] == "done" {
                announce_done(task)?;
            }
        }
        Ok(task)
    }
}
```

The input is a JSON object, so it is a `Map`. A `Some(task)` comes back when the record exists, and `None` (null) when it
does not. Emitting the event is the same as before; `announce_done` is at the end of the file.

<!-- file: rust/tasks/src/lib.rs#72-76 -->
```rust
handler! {
    fn remove_task(input: ById) -> Option<Value> {
        db::delete(MODEL, &input.id)
    }
}
```

<!-- file: rust/tasks/src/lib.rs#78-82 -->
```rust
handler! {
    fn overdue_tasks(_input: Empty) -> Vec<Value> {
        overdue()
    }
}
```

<!-- file: rust/tasks/src/lib.rs#84-88 -->
```rust
handler! {
    fn count_overdue(_input: Empty) -> Value {
        Ok(json!({ "overdue": overdue()?.len() }))
    }
}
```

<!-- file: rust/tasks/src/lib.rs#90-101 -->
```rust
handler! {
    fn finish_task(input: ById) -> Value {
        let task: Value = db::get(MODEL, &input.id)?.ok_or_else(|| Error::msg("No such task."))?;
        if task["status"] != "doing" {
            return Err(Error::msg(i18n::t("task.finish_needs_doing")?));
        }
        let task: Value = db::update(MODEL, &input.id, &json!({ "status": "done" }))?
            .ok_or_else(|| Error::msg("No such task."))?;
        announce_done(&task)?;
        Ok(task)
    }
}
```

<!-- file: rust/tasks/src/lib.rs#104-110 -->
```rust
handler! {
    fn on_task_done(received: events::Received) -> Value {
        let title = received.payload["title"].as_str().unwrap_or("");
        Notification::new("Task done").body(title).level(Level::Info).send()?;
        Ok(json!({}))
    }
}
```

The listener has a type, `events::Received`, for what arrives; the `payload` is the JSON the emitter sent. It runs in the
background as the plugin itself, as in chapter 8.

<!-- file: rust/tasks/src/lib.rs#113-124 -->
```rust
handler! {
    fn mark_overdue(_input: Empty) -> Value {
        let late = overdue()?;
        for task in &late {
            if task["overdue"] != true {
                let id = task["id"].as_str().unwrap_or_default();
                db::update::<Value>(MODEL, id, &json!({ "overdue": true }))?;
            }
        }
        Ok(json!({ "marked": late.len() }))
    }
}
```

Two plain functions are shared. A function outside `handler!` is not exported, which is what `private fn` did in Rhai:

<!-- file: rust/tasks/src/lib.rs#127-134 -->
```rust
fn overdue() -> Result<Vec<Value>> {
    let today = context::current()?.today().to_string();
    db::find::<Value>(MODEL)
        .matching(Filter::lt("due", today).and(Filter::ne("status", "done")))
        .order_by("due")
        .limit(200)
        .all()
}
```

<!-- file: rust/tasks/src/lib.rs#136-141 -->
```rust
fn announce_done(task: &Value) -> Result<()> {
    events::emit(
        "task_done",
        &json!({ "id": task["id"], "title": task["title"], "assignee": task["assignee"] }),
    )
}
```

`Filter::lt(...).and(Filter::ne(...))` is the `{ due: { lt: today }, status: { ne: "done" } }` filter of chapter 7, written
with functions instead of a map.

## Build, install, run

```sh
cd plugins/tasks
make
cd ../..
aether -c aether.toml --load-plugins --dry-run
aether -c aether.toml --upgrade-plugin tasks --org main
aether -c aether.toml --org main --command tasks.hello
```

The check of chapter 10 passes on a compiled plugin too: it also looks for the file `plugin.toml` points at, so it fails
until you have run `make`. After that, add tasks from the pages or the command line, and try the rules of chapters 5 and 6
as a member and as a lead: they answer the same way.

## What is different, in short

| | Rhai | Rust |
|---|---|---|
| Build | none | `make` after each change |
| Reload while developing | on save, with `--watch` | `make`, then `--watch` reloads the built file |
| Fail with a message | `fail("text")` | `Err(Error::msg("text"))` |
| Translated messages | `i18n::t("key")` | `i18n::t("key")?` from the SDK |
| A value the model refuses | the person sees the message (`task.due: must be a date ...`) | the same |
| Data | `db::find(model, #{...})` | `db::find::<T>(model)...all()?` or the `Model` trait, with your own types |
| Not exported | `private fn` | a plain `fn` |
| Test without a server | `tests/smoke.sh` against a running server | the same, and also unit tests against a mock kernel (`aether_sdk::testing::MockKernel`) |
| Libraries | none | any crate that compiles to WebAssembly |

Go back to the [map of the tutorial](README.md), or to [Chapter 11](11-next.md).
