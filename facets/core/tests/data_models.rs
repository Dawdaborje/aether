//! Models against a real SurrealDB (see `host_db_audit.rs` for how to run): applying them to an
//! organization, the kernel's commands through them, and what editing a model does.

use std::collections::HashMap;
use std::sync::Arc;

use aether_core::access::audit::{Actor, AuditContext};
use aether_core::data_model::{
    ModelDef, apply::{apply_plans, plan_models}, schemas_of, sync_ids,
};
use aether_core::kernel::{CallInfo, DbScope, HostError, ModelGrant, PluginHostContext, kernel_command};
use aether_core::notifications::NotificationHub;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_data_models_test";

struct World {
    session: Surreal<Client>,
    org: String,
}

impl World {
    async fn new(label: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Ok(address) = std::env::var("AETHER_TEST_DB") else {
            eprintln!("AETHER_TEST_DB is not set; skipping");
            return Ok(None);
        };
        let db = Surreal::<Client>::init();
        db.connect::<Ws>(address).await?;
        db.signin(Root { username: "root".into(), password: "root".into() }).await?;
        let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.subsec_nanos();
        let org = format!("org_{label}_{suffix}");
        aether_orm::migrate_org(&db, NAMESPACE, &org).await?;
        db.use_ns(NAMESPACE).use_db(&org).await?;
        Ok(Some(Self { session: db, org }))
    }

    /// Apply the models (the upgrade step) and say what it did.
    async fn apply(&self, models: &[ModelDef]) -> Result<(), aether_core::data_model::apply::ApplyError> {
        let plans = plan_models(&self.session, "notes", models).await?;
        apply_plans(&self.session, &plans).await
    }

    /// A call by a plugin that holds these models, with write access.
    fn ctx(&self, models: &[ModelDef]) -> PluginHostContext {
        let grants: HashMap<String, ModelGrant> = schemas_of(models)
            .into_iter()
            .map(|(name, schema)| {
                let mut grant = ModelGrant::from_access(&name, &["read".into(), "write".into()], Some(&schema.table));
                grant.schema = Some(schema);
                (name, grant)
            })
            .collect();
        PluginHostContext::new(
            "notes",
            ["db::query".to_string(), "db::mutate".to_string()].into_iter().collect(),
            grants,
            Arc::new(self.session.clone()),
            DbScope::new(NAMESPACE, &self.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext { actor: Actor::User("users:u1".into()), request_id: "r".into(), ip: None, user_agent: None },
                "f",
            ),
        )
    }

    async fn raw(&self, table: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let mut response = self.session.query(format!("SELECT * FROM {table} ORDER BY id;")).await?.check()?;
        Ok(response.take(0)?)
    }
}

fn note() -> ModelDef {
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "note",
        "fields": [
            { "name": "title", "type": "string", "required": true, "max_length": 20, "index": "unique" },
            { "name": "body", "type": "text" },
            { "name": "status", "type": "select", "default": "draft",
              "options": [{ "value": "draft" }, { "value": "shared" }] },
            { "name": "pages", "type": "int" }
        ]
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    sync_ids(&mut model);
    model
}

fn id_of(model: &ModelDef, name: &str) -> String {
    model.fields.iter().find(|f| f.name == name).and_then(|f| f.id.clone()).unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn records_are_stored_under_field_ids_and_come_back_under_names() -> TestResult {
    let Some(world) = World::new("store").await? else { return Ok(()) };
    let model = note();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));

    let created = kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "title": "Plan", "pages": 3 } })).await?;
    let record = &created["data"];
    assert_eq!(record["title"], "Plan");
    assert_eq!(record["status"], "draft", "the default was applied");
    assert!(record["id"].as_str().is_some_and(|id| id.starts_with(model.model_id.as_deref().unwrap_or("?"))));

    // In the database only ids are used.
    let rows = world.raw(model.model_id.as_deref().unwrap_or_default()).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][id_of(&model, "title")], "Plan");
    assert!(rows[0].get("title").is_none(), "no field name is ever stored");

    let found = kernel_command(&ctx, "db::find", json!({ "model": "note", "filter": { "status": "draft" }, "order": "title" })).await?;
    assert_eq!(found["data"][0]["title"], "Plan");
    let updated = kernel_command(&ctx, "db::update", json!({ "model": "note", "id": record["id"], "data": { "pages": 4, "body": "text" } })).await?;
    assert_eq!(updated["data"]["pages"], 4);
    // Null clears an optional field.
    let cleared = kernel_command(&ctx, "db::update", json!({ "model": "note", "id": record["id"], "data": { "body": null } })).await?;
    assert!(cleared["data"].get("body").is_none() || cleared["data"]["body"].is_null());
    let after = world.raw(model.model_id.as_deref().unwrap_or_default()).await?;
    assert!(after[0].get(id_of(&model, "body")).is_none_or(Value::is_null), "the value is gone from the record");
    let gone = kernel_command(&ctx, "db::delete", json!({ "model": "note", "id": record["id"] })).await?;
    assert_eq!(gone["data"]["title"], "Plan");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn the_model_refuses_what_it_does_not_allow() -> TestResult {
    let Some(world) = World::new("rules").await? else { return Ok(()) };
    let model = note();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let create = |data: Value| {
        let ctx = &ctx;
        async move { kernel_command(ctx, "db::create", json!({ "model": "note", "data": data })).await }
    };

    for bad in [
        json!({ "titel": "typo" }),
        json!({ "body": "no title" }),
        json!({ "title": "this title is far too long" }),
        json!({ "title": "x", "status": "archived" }),
        json!({ "title": "x", "pages": "three" }),
    ] {
        let result = create(bad.clone()).await;
        assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{bad}: {result:?}");
    }
    // The schema is also enforced by the database: a unique title cannot repeat.
    create(json!({ "title": "same" })).await?;
    assert!(create(json!({ "title": "same" })).await.is_err(), "the unique index refuses a repeat");
    // Filters and ordering use names too, and refuse unknown ones.
    let filtered = kernel_command(&ctx, "db::find", json!({ "model": "note", "filter": { "nope": 1 } })).await;
    assert!(matches!(filtered, Err(HostError::InvalidPayload(_))));
    let ordered = kernel_command(&ctx, "db::find", json!({ "model": "note", "order": "nope" })).await;
    assert!(matches!(ordered, Err(HostError::InvalidPayload(_))));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn renaming_and_hiding_fields_and_adding_new_ones_moves_no_data() -> TestResult {
    let Some(world) = World::new("rename").await? else { return Ok(()) };
    let before = note();
    world.apply(std::slice::from_ref(&before)).await?;
    let ctx = world.ctx(std::slice::from_ref(&before));
    for title in ["One", "Two", "Three"] {
        kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "title": title, "body": "b", "pages": 1 } })).await?;
    }
    let table = before.model_id.clone().unwrap_or_default();
    let stored_before = world.raw(&table).await?;

    // Rename `title`, hide `pages`, add `tags`, relabel the model: all at once.
    let mut after = before.clone();
    after.fields[0].name = "heading".into();
    after.fields[0].label = Some("Heading".into());
    after.fields[3].deprecated = true;
    after.label = Some("Renamed".into());
    after.fields.push(serde_json::from_value(json!({ "name": "tags", "type": "string" }))?);
    sync_ids(&mut after);
    let plans = plan_models(&world.session, "notes", std::slice::from_ref(&after)).await?;
    assert!(plans[0].0.blockers.is_empty());
    assert!(plans[0].0.ops.iter().all(|op| !matches!(op, aether_core::data_model::apply::Op::Backfill { .. })), "no record is rewritten");
    apply_plans(&world.session, &plans).await?;

    // Every stored record is exactly as it was.
    assert_eq!(world.raw(&table).await?, stored_before, "nothing was copied, renamed or rewritten");

    // Through the new definition the same records read under the new names; the hidden field
    // is gone from the plugin's point of view but its data is still there.
    let ctx = world.ctx(std::slice::from_ref(&after));
    let found = kernel_command(&ctx, "db::find", json!({ "model": "note", "order": "heading" })).await?;
    let rows = found["data"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.iter().map(|r| r["heading"].as_str().unwrap_or("")).collect::<Vec<_>>(), ["One", "Three", "Two"]);
    assert!(rows.iter().all(|row| row.get("title").is_none() && row.get("pages").is_none()));
    assert!(world.raw(&table).await?.iter().all(|row| row.get(id_of(&before, "pages")).is_some()), "hidden data is kept");
    assert!(matches!(
        kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "title": "old name" } })).await,
        Err(HostError::InvalidPayload(_))
    ));
    kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "heading": "Four", "tags": "a" } })).await?;

    // Applying the same thing again is a no-op.
    let again = plan_models(&world.session, "notes", std::slice::from_ref(&after)).await?;
    assert!(again[0].0.is_noop(again[0].1.as_ref()), "{:?}", again[0].0.ops);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn unsafe_changes_are_blocked_before_anything_is_touched() -> TestResult {
    let Some(world) = World::new("blocked").await? else { return Ok(()) };
    let before = note();
    world.apply(std::slice::from_ref(&before)).await?;
    let ctx = world.ctx(std::slice::from_ref(&before));
    kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "title": "One", "pages": 2 } })).await?;
    let table = before.model_id.clone().unwrap_or_default();
    let stored = world.raw(&table).await?;

    // A new required field with no default, over existing records.
    let mut required = before.clone();
    required.fields.push(serde_json::from_value(json!({ "name": "owner", "type": "string", "required": true }))?);
    sync_ids(&mut required);
    let blocked = world.apply(std::slice::from_ref(&required)).await;
    assert!(
        matches!(&blocked, Err(aether_core::data_model::apply::ApplyError::Blocked { problems, .. }) if problems.iter().any(|p| p.contains("owner"))),
        "{blocked:?}"
    );
    // A lossy type change.
    let mut narrowed = before.clone();
    narrowed.fields[3].kind = aether_core::data_model::FieldType::Bool;
    assert!(world.apply(std::slice::from_ref(&narrowed)).await.is_err());
    assert_eq!(world.raw(&table).await?, stored, "a blocked upgrade changes nothing");

    // With a default the required field is allowed, and existing records get it.
    required.fields[4].default = Some(json!("nobody"));
    world.apply(std::slice::from_ref(&required)).await?;
    let owner = id_of(&required, "owner");
    assert!(world.raw(&table).await?.iter().all(|row| row[owner.as_str()] == "nobody"), "records were backfilled");
    // A widening is fine: int to float.
    let mut widened = required.clone();
    widened.fields[3].kind = aether_core::data_model::FieldType::Float;
    world.apply(std::slice::from_ref(&widened)).await?;
    Ok(())
}

fn ticket() -> ModelDef {
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "ticket",
        "chatter": { "enabled": true },
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "status", "type": "select", "default": "open", "track": true,
              "options": [{ "value": "open" }, { "value": "closed" }] },
            { "name": "points", "type": "int", "track": true },
            { "name": "notes", "type": "text" }
        ]
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    sync_ids(&mut model);
    model
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn chatter_records_creation_tracked_changes_and_deletion_with_the_write() -> TestResult {
    let Some(world) = World::new("chatter").await? else { return Ok(()) };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let table = model.model_id.clone().unwrap_or_default();

    let created = kernel_command(&ctx, "db::create", json!({ "model": "ticket", "data": { "title": "Fix" } })).await?;
    let id = created["data"]["id"].clone();
    let lines = world.raw("chatter_messages").await?;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "system");
    assert_eq!(lines[0]["author"], "users:u1");
    assert_eq!(lines[0]["model_id"], table.as_str());
    let key = lines[0]["record_key"].as_str().unwrap_or_default().to_string();
    assert!(id.as_str().is_some_and(|id| id.ends_with(&key)), "the thread is keyed by the record's key");
    assert_eq!(world.raw("chatter_followers").await?.len(), 1, "the creator follows");

    // An untracked field changes nothing in the thread; a tracked one is recorded as before/after.
    kernel_command(&ctx, "db::update", json!({ "model": "ticket", "id": id, "data": { "notes": "x" } })).await?;
    assert_eq!(world.raw("chatter_messages").await?.len(), 1);
    kernel_command(&ctx, "db::update", json!({ "model": "ticket", "id": id, "data": { "status": "closed", "points": 3, "notes": "y" } })).await?;
    let lines = world.raw("chatter_messages").await?;
    assert_eq!(lines.len(), 2);
    let change = &lines[1];
    assert_eq!(change["kind"], "change");
    assert_eq!(change["before"][id_of(&model, "status")], "open");
    assert_eq!(change["after"][id_of(&model, "status")], "closed");
    assert_eq!(change["after"][id_of(&model, "points")], 3);
    assert!(change["after"].get(id_of(&model, "notes")).is_none(), "only tracked fields");

    // Writing the same value again is not a change.
    kernel_command(&ctx, "db::update", json!({ "model": "ticket", "id": id, "data": { "status": "closed" } })).await?;
    assert_eq!(world.raw("chatter_messages").await?.len(), 2);

    kernel_command(&ctx, "db::delete", json!({ "model": "ticket", "id": id })).await?;
    let lines = world.raw("chatter_messages").await?;
    assert!(lines.iter().all(|line| line["record_deleted_at"].is_string()), "the thread waits in the trash");
    assert!(world.raw("chatter_followers").await?.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_model_without_chatter_writes_nothing_to_it() -> TestResult {
    let Some(world) = World::new("nochatter").await? else { return Ok(()) };
    let model = note();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let created = kernel_command(&ctx, "db::create", json!({ "model": "note", "data": { "title": "A" } })).await?;
    kernel_command(&ctx, "db::update", json!({ "model": "note", "id": created["data"]["id"], "data": { "pages": 2 } })).await?;
    kernel_command(&ctx, "db::delete", json!({ "model": "note", "id": created["data"]["id"] })).await?;
    assert!(world.raw("chatter_messages").await?.is_empty());
    Ok(())
}

const SCRIPT: &str = r#"
    fn add(input) {
        let ticket = db::create("ticket", #{ title: input.title });
        ticket
    }
    fn titles() {
        let rows = db::find("ticket", #{ order: "title" });
        rows.map(|row| row.title)
    }
    fn close(input) {
        let row = db::get("ticket", input.id);
        if row == () { fail("no such ticket"); }
        db::update("ticket", input.id, #{ status: "closed" })
    }
    fn bad() { db::create("ticket", #{ nonsense: 1 }) }
    fn who() { context::get().actor }
"#;

async fn run(
    program: &Arc<aether_core::plugin_manager::script::ScriptProgram>,
    ctx: &PluginHostContext,
    function: &'static str,
    input: Value,
) -> Result<Value, aether_core::plugin_manager::script::ScriptError> {
    let (program, ctx) = (program.clone(), ctx.clone());
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || program.call(function, input, ctx, runtime))
        .await
        .unwrap_or_else(|error| panic!("{error}"))
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_rhai_plugin_uses_the_same_models_grants_and_chatter() -> TestResult {
    use aether_core::plugin_manager::script::{ScriptError, ScriptProgram};
    let Some(world) = World::new("rhai").await? else { return Ok(()) };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let program = Arc::new(ScriptProgram::compile(SCRIPT)?);

    let created = run(&program, &ctx, "add", json!({ "title": "From a script" })).await?;
    assert_eq!(created["title"], "From a script");
    assert_eq!(created["status"], "open", "defaults apply");
    assert_eq!(run(&program, &ctx, "titles", Value::Null).await?, json!(["From a script"]));
    assert_eq!(run(&program, &ctx, "who", Value::Null).await?["id"], "users:u1");

    // The model still refuses what it does not allow, and the failure is internal.
    assert!(matches!(run(&program, &ctx, "bad", Value::Null).await, Err(ScriptError::Failed(_))));

    // `fail` reaches the caller; a change made by a script is tracked like any other.
    let closed = run(&program, &ctx, "close", json!({ "id": created["id"] })).await?;
    assert_eq!(closed["status"], "closed");
    let lines = world.raw("chatter_messages").await?;
    assert!(lines.iter().any(|line| line["kind"] == "change"));
    let missing = run(&program, &ctx, "close", json!({ "id": "nope" })).await;
    assert!(matches!(missing, Err(ScriptError::User(ref message)) if message == "no such ticket"), "{missing:?}");

    // Without the capability the script is refused, as a WASM plugin would be.
    let mut read_only = ctx.clone();
    read_only.granted_capabilities.remove("db::mutate");
    assert!(matches!(run(&program, &read_only, "add", json!({ "title": "x" })).await, Err(ScriptError::Failed(_))));
    Ok(())
}
