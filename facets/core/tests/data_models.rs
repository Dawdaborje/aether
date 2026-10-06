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
    // Record ids are random, so the rows come back in no particular order.
    let change = lines.iter().find(|line| line["kind"] == "change").ok_or("no change line")?;
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

const LUA_SCRIPT: &str = r#"
    function add(input)
        return db.create("ticket", { title = input.title })
    end
    function titles()
        local titles = {}
        for _, row in ipairs(db.find("ticket", { order = "title" })) do
            titles[#titles + 1] = row.title
        end
        return titles
    end
    function close(input)
        local row = db.get("ticket", input.id)
        if row == nil then fail("no such ticket") end
        return db.update("ticket", input.id, { status = "closed" })
    end
    function bad() return db.create("ticket", { nonsense = 1 }) end
    function who() return context.get().actor end
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
    script_plugin_uses_the_same_models_grants_and_chatter("rhai", aether_core::plugin_manager::script::ScriptKind::Rhai, SCRIPT).await
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_lua_plugin_uses_the_same_models_grants_and_chatter() -> TestResult {
    script_plugin_uses_the_same_models_grants_and_chatter("lua", aether_core::plugin_manager::script::ScriptKind::Lua, LUA_SCRIPT).await
}

async fn script_plugin_uses_the_same_models_grants_and_chatter(label: &str, kind: aether_core::plugin_manager::script::ScriptKind, source: &str) -> TestResult {
    use aether_core::plugin_manager::script::{ScriptError, ScriptProgram};
    let Some(world) = World::new(label).await? else { return Ok(()) };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let program = Arc::new(ScriptProgram::compile(kind, source)?);

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

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_unique_multi_field_index_refuses_duplicates_and_follows_renames() -> TestResult {
    let Some(world) = World::new("composite").await? else { return Ok(()) };
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "assignment",
        "fields": [
            { "name": "person", "type": "string" },
            { "name": "post", "type": "string" },
            { "name": "note", "type": "string" }
        ],
        "indexes": [ { "fields": ["person", "post"], "unique": true } ]
    }))?;
    sync_ids(&mut model);
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));

    let row = |person: &str, post: &str| json!({ "model": "assignment", "data": { "person": person, "post": post } });
    kernel_command(&ctx, "db::create", row("ann", "clerk")).await?;
    kernel_command(&ctx, "db::create", row("ann", "chief")).await?;
    kernel_command(&ctx, "db::create", row("bob", "clerk")).await?;
    let duplicate = kernel_command(&ctx, "db::create", row("ann", "clerk")).await;
    assert!(matches!(duplicate, Err(HostError::Db(_))), "{duplicate:?}");

    // Renaming a field is a refresh of the comment only: the index stays and still holds.
    model.fields[0].name = "employee".into();
    model.indexes[0].fields[0] = "employee".into();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let again = kernel_command(&ctx, "db::create", json!({ "model": "assignment", "data": { "employee": "bob", "post": "clerk" } })).await;
    assert!(matches!(again, Err(HostError::Db(_))), "{again:?}");

    // Dropping the index lets the duplicate in.
    model.indexes.clear();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    kernel_command(&ctx, "db::create", json!({ "model": "assignment", "data": { "employee": "bob", "post": "clerk" } })).await?;
    Ok(())
}

fn org_models() -> Vec<ModelDef> {
    let mut models: Vec<ModelDef> = serde_json::from_value(json!([
        { "name": "unit", "fields": [
            { "name": "name", "type": "string", "required": true },
            { "name": "parent", "type": "link", "target": "unit", "hierarchy": true },
            { "name": "skills", "type": "many2many", "target": "skill" }
        ] },
        { "name": "skill", "fields": [ { "name": "name", "type": "string" } ] }
    ]))
    .unwrap_or_else(|error| panic!("{error}"));
    for model in &mut models {
        sync_ids(model);
    }
    models
}

fn names(reply: &Value) -> Vec<String> {
    let mut found: Vec<String> = reply["data"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    found.sort();
    found
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_hierarchy_keeps_its_edges_refuses_loops_and_walks_any_depth() -> TestResult {
    let Some(world) = World::new("tree").await? else { return Ok(()) };
    let models = org_models();
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    let make = |name: &str, parent: Option<&str>| {
        let mut data = json!({ "name": name });
        if let Some(parent) = parent {
            data["parent"] = json!(parent);
        }
        json!({ "model": "unit", "data": data })
    };
    let ministry = kernel_command(&ctx, "db::create", make("ministry", None)).await?["data"]["id"].as_str().unwrap_or_default().to_string();
    let dept = kernel_command(&ctx, "db::create", make("dept", Some(&ministry))).await?["data"]["id"].as_str().unwrap_or_default().to_string();
    let unit = kernel_command(&ctx, "db::create", make("unit", Some(&dept))).await?["data"]["id"].as_str().unwrap_or_default().to_string();
    let other = kernel_command(&ctx, "db::create", make("other", Some(&ministry))).await?["data"]["id"].as_str().unwrap_or_default().to_string();

    let tree = |id: &str, direction: &str, depth: Option<u32>, include_self: bool| {
        let mut payload = json!({ "model": "unit", "field": "parent", "id": id, "direction": direction, "include_self": include_self });
        if let Some(depth) = depth {
            payload["depth"] = json!(depth);
        }
        kernel_command(&ctx, "db::tree", payload)
    };
    assert_eq!(names(&tree(&ministry, "down", None, false).await?), ["dept", "other", "unit"]);
    assert_eq!(names(&tree(&ministry, "down", Some(1), false).await?), ["dept", "other"]);
    assert_eq!(names(&tree(&ministry, "down", None, true).await?), ["dept", "ministry", "other", "unit"]);
    assert_eq!(names(&tree(&unit, "up", None, false).await?), ["dept", "ministry"]);
    assert!(names(&tree(&other, "down", None, false).await?).is_empty());

    // A parent that does not exist, a loop, and a record under itself are all refused...
    let missing = kernel_command(&ctx, "db::create", make("ghost", Some(&format!("{}:nothing", ministry.split(':').next().unwrap_or_default())))).await;
    assert!(matches!(missing, Err(HostError::InvalidPayload(ref m)) if m.contains("does not exist")), "{missing:?}");
    for (record, parent) in [(&ministry, &unit), (&ministry, &ministry), (&dept, &unit)] {
        let looped = kernel_command(&ctx, "db::update", json!({ "model": "unit", "id": record, "data": { "parent": parent } })).await;
        assert!(matches!(looped, Err(HostError::InvalidPayload(ref m)) if m.contains("loop")), "{looped:?}");
    }
    // ...and a failed move changes nothing.
    assert_eq!(names(&tree(&ministry, "down", None, false).await?), ["dept", "other", "unit"]);

    // Moving a subtree is one write; the walk follows at once.
    kernel_command(&ctx, "db::update", json!({ "model": "unit", "id": dept, "data": { "parent": other } })).await?;
    assert_eq!(names(&tree(&other, "down", None, false).await?), ["dept", "unit"]);
    assert_eq!(names(&tree(&unit, "up", None, false).await?), ["dept", "ministry", "other"]);
    // Clearing the parent makes a root.
    kernel_command(&ctx, "db::update", json!({ "model": "unit", "id": dept, "data": { "parent": null } })).await?;
    assert!(names(&tree(&other, "down", None, false).await?).is_empty());
    assert_eq!(names(&tree(&dept, "down", None, false).await?), ["unit"]);

    // A record with children is not deleted; a leaf is.
    let blocked = kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": dept })).await;
    assert!(matches!(blocked, Err(HostError::InvalidPayload(ref m)) if m.contains("children")), "{blocked:?}");
    kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": unit })).await?;
    kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": dept })).await?;
    assert_eq!(names(&tree(&ministry, "down", None, false).await?), ["other"]);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn many_to_many_fields_are_edges_read_both_ways() -> TestResult {
    let Some(world) = World::new("m2m").await? else { return Ok(()) };
    let models = org_models();
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    let id = |reply: Value| reply["data"]["id"].as_str().unwrap_or_default().to_string();
    let unit = id(kernel_command(&ctx, "db::create", json!({ "model": "unit", "data": { "name": "u" } })).await?);
    let other = id(kernel_command(&ctx, "db::create", json!({ "model": "unit", "data": { "name": "o" } })).await?);
    let rust = id(kernel_command(&ctx, "db::create", json!({ "model": "skill", "data": { "name": "rust" } })).await?);
    let go = id(kernel_command(&ctx, "db::create", json!({ "model": "skill", "data": { "name": "go" } })).await?);

    kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [rust, go] })).await?;
    kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": other, "to": [rust] })).await?;
    // Again is harmless.
    kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [rust] })).await?;

    let skills = kernel_command(&ctx, "db::related", json!({ "model": "unit", "field": "skills", "id": unit })).await?;
    assert_eq!(names(&skills), ["go", "rust"]);
    let who = kernel_command(&ctx, "db::related", json!({ "model": "unit", "field": "skills", "id": rust, "reverse": true })).await?;
    assert_eq!(names(&who), ["o", "u"]);

    kernel_command(&ctx, "db::unrelate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [go] })).await?;
    let skills = kernel_command(&ctx, "db::related", json!({ "model": "unit", "field": "skills", "id": unit })).await?;
    assert_eq!(names(&skills), ["rust"]);

    // Wrong target model, a record that is not there, a field that is not a relation: all refused.
    let wrong = kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [other] })).await;
    assert!(matches!(wrong, Err(HostError::InvalidPayload(_))), "{wrong:?}");
    let table = rust.split(':').next().unwrap_or_default();
    let ghost = kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [format!("{table}:nothing")] })).await;
    assert!(matches!(ghost, Err(HostError::InvalidPayload(ref m)) if m.contains("does not exist")), "{ghost:?}");
    let not_relation = kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "name", "id": unit, "to": [rust] })).await;
    assert!(matches!(not_relation, Err(HostError::InvalidPayload(_))), "{not_relation:?}");
    // A many2many field is not a value either.
    let as_value = kernel_command(&ctx, "db::update", json!({ "model": "unit", "id": unit, "data": { "skills": [rust] } })).await;
    assert!(matches!(as_value, Err(HostError::InvalidPayload(_))), "{as_value:?}");

    // Deleting a record takes its edges with it.
    kernel_command(&ctx, "db::delete", json!({ "model": "skill", "id": rust })).await?;
    let skills = kernel_command(&ctx, "db::related", json!({ "model": "unit", "field": "skills", "id": unit })).await?;
    assert!(names(&skills).is_empty());
    Ok(())
}

mod rules_enforcement {
    use std::sync::Arc;

    use aether_core::data_model::RuleSet;
    use aether_core::kernel::PluginCaller;

    use super::*;

    /// Answers the plugin's one rule variable: the people on "ann"'s team.
    struct Variables;

    #[async_trait::async_trait]
    impl PluginCaller for Variables {
        async fn call(&self, _plugin: &str, function: &str, _payload: Value, _trail: Vec<String>) -> Result<Value, HostError> {
            match function {
                "rule_var_team" => Ok(json!(["users:bob"])),
                "rule_var_nobody" => Ok(Value::Null),
                other => Err(HostError::Message(format!("no variable {other}"))),
            }
        }
    }

    fn ticket_model() -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "ticket",
            "fields": [
                { "name": "owner", "type": "string", "required": true },
                { "name": "title", "type": "string" },
                { "name": "state", "type": "select", "default": "submitted",
                  "options": [{ "value": "submitted" }, { "value": "approved" }] },
                { "name": "notes", "type": "text" }
            ]
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        sync_ids(&mut model);
        model
    }

    fn rules() -> RuleSet {
        RuleSet::parse(
            &json!({
                "model": "ticket",
                "access": [
                    { "name": "own", "operations": ["read", "create", "write", "delete"], "when": { "owner": "$user" } },
                    { "name": "team", "roles": ["lead"], "operations": ["read"], "when": { "owner": { "in": "$team" } } },
                    { "name": "approvers", "roles": ["approver"], "operations": ["read", "write"] }
                ],
                "restrict": [
                    { "name": "pending_only", "operations": ["write", "delete"], "exempt_roles": ["approver"],
                      "when": { "state": "submitted" } }
                ],
                "fields": [
                    { "fields": ["state"], "write_roles": ["approver"] },
                    { "fields": ["notes"], "read_roles": ["approver"] }
                ]
            })
            .to_string(),
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }

    async fn as_user(world: &World, model: &ModelDef, user: &str, roles: &[&str]) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
        // The user is a member of the organization with these roles.
        let mut give = String::new();
        for role in roles {
            give.push_str(&format!(
                "UPSERT roles SET name = '{role}', label = '{role}' WHERE name = '{role}'; \
                 LET $u = (SELECT VALUE id FROM org_users WHERE core_user_id = '{user}' LIMIT 1)[0]; \
                 LET $r = (SELECT VALUE id FROM roles WHERE name = '{role}' LIMIT 1)[0]; \
                 UPSERT org_user_roles SET org_user = $u, role = $r WHERE org_user = $u AND role = $r;"
            ));
        }
        world
            .session
            .query(format!(
                "UPSERT org_users SET core_user_id = '{user}', display_name = '{user}', is_active = true WHERE core_user_id = '{user}'; {give}"
            ))
            .await?
            .check()?;
        let grants: HashMap<String, ModelGrant> = schemas_of(std::slice::from_ref(model))
            .into_iter()
            .map(|(name, schema)| {
                let mut grant = ModelGrant::from_access(&name, &["read".into(), "write".into()], Some(&schema.table));
                grant.schema = Some(schema);
                grant.rules = Some(Arc::new(rules()));
                (name, grant)
            })
            .collect();
        Ok(PluginHostContext::new(
            "desk",
            ["db::query".to_string(), "db::mutate".to_string()].into_iter().collect(),
            grants,
            Arc::new(world.session.clone()),
            DbScope::new(NAMESPACE, &world.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext { actor: Actor::User(user.into()), request_id: "r".into(), ip: None, user_agent: None },
                "f",
            ),
        )
        .with_plugin_calls(Vec::new(), Vec::new(), Arc::new(Variables)))
    }

    fn titles(reply: &Value) -> Vec<String> {
        let mut found: Vec<String> = reply["data"]
            .as_array()
            .map(|rows| rows.iter().filter_map(|r| r["title"].as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        found.sort();
        found
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn rules_decide_who_sees_and_changes_what() -> TestResult {
        let Some(world) = World::new("rules").await? else { return Ok(()) };
        let model = ticket_model();
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &["desk.lead"]).await?;
        let bob = as_user(&world, &model, "users:bob", &[]).await?;
        let cy = as_user(&world, &model, "users:cy", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        let root = as_user(&world, &model, "users:root", &["org_admin"]).await?;

        async fn make(ctx: &PluginHostContext, owner: &str, title: &str) -> Result<Value, HostError> {
            kernel_command(ctx, "db::create", json!({ "model": "ticket", "data": { "owner": owner, "title": title, "notes": "private" } })).await
        }
        // Everyone can create their own; nobody can create someone else's.
        let ann_ticket = make(&ann, "users:ann", "ann-1").await?["data"]["id"].as_str().unwrap_or_default().to_string();
        let bob_ticket = make(&bob, "users:bob", "bob-1").await?["data"]["id"].as_str().unwrap_or_default().to_string();
        make(&cy, "users:cy", "cy-1").await?;
        let forged = make(&bob, "users:cy", "forged").await;
        assert!(matches!(forged, Err(HostError::Denied(_))), "{forged:?}");
        assert_eq!(titles(&kernel_command(&root, "db::find", json!({ "model": "ticket" })).await?).len(), 3, "the refused create left nothing");

        // Reading: your own; a lead also sees the team (the variable); an approver sees all; an admin all.
        let all = json!({ "model": "ticket" });
        assert_eq!(titles(&kernel_command(&bob, "db::find", all.clone()).await?), ["bob-1"]);
        assert_eq!(titles(&kernel_command(&ann, "db::find", all.clone()).await?), ["ann-1", "bob-1"]);
        assert_eq!(titles(&kernel_command(&carol, "db::find", all.clone()).await?), ["ann-1", "bob-1", "cy-1"]);
        assert_eq!(titles(&kernel_command(&root, "db::find", all.clone()).await?).len(), 3);
        assert_eq!(kernel_command(&bob, "db::count", all.clone()).await?["data"], 1);
        assert_eq!(kernel_command(&carol, "db::count", all.clone()).await?["data"], 3);
        // A record you may not read is as if it were not there.
        let cy_ticket = kernel_command(&root, "db::find", json!({ "model": "ticket", "filter": { "owner": "users:cy" } })).await?["data"][0]["id"]
            .as_str().unwrap_or_default().to_string();
        assert!(kernel_command(&bob, "db::get", json!({ "model": "ticket", "id": cy_ticket })).await?["data"].is_null());
        assert_eq!(kernel_command(&bob, "db::get", json!({ "model": "ticket", "id": bob_ticket })).await?["data"]["title"], "bob-1");

        // Fields: notes are for approvers only; the state is theirs to set.
        let mine = kernel_command(&bob, "db::get", json!({ "model": "ticket", "id": bob_ticket })).await?;
        assert!(mine["data"].get("notes").is_none(), "hidden field: {mine}");
        let approver_view = kernel_command(&carol, "db::get", json!({ "model": "ticket", "id": bob_ticket })).await?;
        assert_eq!(approver_view["data"]["notes"], "private");
        let search_hidden = kernel_command(&bob, "db::find", json!({ "model": "ticket", "filter": { "notes": "private" } })).await;
        assert!(matches!(search_hidden, Err(HostError::Denied(_))), "{search_hidden:?}");
        let self_approve = kernel_command(&bob, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "state": "approved" } })).await;
        assert!(matches!(self_approve, Err(HostError::Denied(_))), "{self_approve:?}");

        // Writing: your own while pending; never someone else's; approvers any, any time.
        kernel_command(&bob, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "bob-1b" } })).await?;
        let others = kernel_command(&bob, "db::update", json!({ "model": "ticket", "id": cy_ticket, "data": { "title": "x" } })).await;
        assert!(matches!(others, Err(HostError::Denied(_))), "{others:?}");
        let lead_write = kernel_command(&ann, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "x" } })).await;
        assert!(matches!(lead_write, Err(HostError::Denied(_))), "a lead may read the team, not change it: {lead_write:?}");
        kernel_command(&carol, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "state": "approved" } })).await?;
        // Once approved, the owner can no longer change or delete it.
        let after = kernel_command(&bob, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "late" } })).await;
        assert!(matches!(after, Err(HostError::Denied(_))), "{after:?}");
        let removed = kernel_command(&bob, "db::delete", json!({ "model": "ticket", "id": bob_ticket })).await;
        assert!(matches!(removed, Err(HostError::Denied(_))), "{removed:?}");
        // ...but a pending one of their own can be deleted, and an approver can still change the approved one.
        kernel_command(&cy, "db::delete", json!({ "model": "ticket", "id": cy_ticket })).await?;
        kernel_command(&carol, "db::update", json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "approved-1" } })).await?;
        assert_eq!(titles(&kernel_command(&root, "db::find", all).await?), ["ann-1", "approved-1"]);
        let _ = ann_ticket;
        Ok(())
    }
}

fn invoice_models() -> Result<Vec<ModelDef>, Box<dyn std::error::Error>> {
    let mut customer: ModelDef = serde_json::from_value(json!({
        "name": "customer",
        "fields": [{ "name": "name", "type": "string", "required": true }]
    }))?;
    let mut invoice: ModelDef = serde_json::from_value(json!({
        "name": "invoice",
        "fields": [
            { "name": "number", "type": "string", "index": "unique",
              "sequence": { "pattern": "INV-{YYYY}-{###}", "reset": "yearly" } },
            { "name": "customer", "type": "link", "target": "customer" },
            { "name": "amount", "type": "decimal", "scale": 2, "min": "0.01", "max": "1000" },
            { "name": "status", "type": "select", "default": "draft",
              "options": [{ "value": "draft" }, { "value": "paid" }] },
            { "name": "paid_on", "type": "date" },
            { "name": "start", "type": "date" },
            { "name": "end", "type": "date" }
        ],
        "checks": [
            { "require": { "or": [ { "status": { "ne": "paid" } }, { "paid_on": { "null": false } } ] },
              "message": "a paid invoice needs the date it was paid" },
            { "require": { "or": [ { "end": { "null": true } }, { "end": { "gte": { "field": "start" } } } ] },
              "message": "the end is before the start" }
        ]
    }))?;
    sync_ids(&mut customer);
    // The invoice links to the customer by the id the customer was just given.
    sync_ids(&mut invoice);
    Ok(vec![customer, invoice])
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn numbering_checks_limits_and_link_existence_hold_inside_the_write() -> TestResult {
    let Some(world) = World::new("integrity").await? else { return Ok(()) };
    let models = invoice_models()?;
    world.apply(&models).await?;
    let ctx = world.ctx(&models);
    let year = chrono::Datelike::year(&chrono::Utc::now());

    let customer = kernel_command(&ctx, "db::create", json!({ "model": "customer", "data": { "name": "Acme" } })).await?;
    let customer_id = customer["data"]["id"].clone();

    // Numbers come from the series, in order, and the created record shows its number.
    let first = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "customer": customer_id, "amount": "10.00" } })).await?;
    assert_eq!(first["data"]["number"], format!("INV-{year}-001"));
    let second = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "amount": "20" } })).await?;
    assert_eq!(second["data"]["number"], format!("INV-{year}-002"));

    // A create that fails gives its number back: nothing is skipped.
    let refused = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "status": "paid" } })).await;
    assert!(matches!(&refused, Err(HostError::InvalidPayload(m)) if m.contains("needs the date it was paid")), "{refused:?}");
    let third = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "amount": "30" } })).await?;
    assert_eq!(third["data"]["number"], format!("INV-{year}-003"), "the refused create used no number");

    // A number given by the caller is kept and takes nothing from the series.
    let given = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "number": "MANUAL-1" } })).await?;
    assert_eq!(given["data"]["number"], "MANUAL-1");
    let fourth = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "amount": "1" } })).await?;
    assert_eq!(fourth["data"]["number"], format!("INV-{year}-004"));

    // Limits.
    let low = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "amount": "0" } })).await;
    assert!(low.is_err(), "{low:?}");
    let high = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": first["data"]["id"], "data": { "amount": "1000.01" } })).await;
    assert!(high.is_err(), "{high:?}");

    // Checks on update, including one field against another.
    let id = first["data"]["id"].clone();
    let paid = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "status": "paid" } })).await;
    assert!(matches!(&paid, Err(HostError::InvalidPayload(m)) if m.contains("needs the date")), "{paid:?}");
    kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "status": "paid", "paid_on": "2026-10-01" } })).await?;
    let backwards = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "start": "2026-10-05", "end": "2026-10-01" } })).await;
    assert!(matches!(&backwards, Err(HostError::InvalidPayload(m)) if m.contains("end is before the start")), "{backwards:?}");
    kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "start": "2026-10-01", "end": "2026-10-05" } })).await?;
    let after = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after["data"]["end"], "2026-10-05", "a refused write left nothing behind");

    // A link must point at a record that exists.
    let ghost = format!("{}:doesnotexist", models[0].model_id.clone().unwrap_or_default());
    let dangling = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "customer": ghost } })).await;
    assert!(matches!(&dangling, Err(HostError::InvalidPayload(m)) if m.contains("does not exist")), "{dangling:?}");
    let dangling = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "customer": ghost } })).await;
    assert!(dangling.is_err(), "{dangling:?}");
    kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "customer": customer_id } })).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn calculated_fields_are_filled_exactly_on_every_write_and_cannot_be_written() -> TestResult {
    let Some(world) = World::new("derived").await? else { return Ok(()) };
    let mut customer: ModelDef = serde_json::from_value(json!({
        "name": "customer",
        "fields": [{ "name": "name", "type": "string", "required": true }]
    }))?;
    let mut line: ModelDef = serde_json::from_value(json!({
        "name": "line",
        "fields": [
            { "name": "customer", "type": "link", "target": "customer" },
            { "name": "customer_name", "type": "string", "related": "customer.name" },
            { "name": "qty", "type": "int" },
            { "name": "price", "type": "decimal", "scale": 2 },
            { "name": "discount", "type": "decimal", "scale": 2 },
            { "name": "subtotal", "type": "decimal", "scale": 2, "compute": "qty * price" },
            { "name": "total", "type": "decimal", "scale": 2, "compute": "subtotal - discount" },
            { "name": "vat", "type": "decimal", "scale": 2, "compute": "total * 0.075" }
        ]
    }))?;
    sync_ids(&mut customer);
    sync_ids(&mut line);
    let models = vec![customer, line];
    for model in &models {
        assert!(model.problems().is_empty(), "{:?}", model.problems());
    }
    aether_core::data_model::definition::validate_set(&models)?;
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    let acme = kernel_command(&ctx, "db::create", json!({ "model": "customer", "data": { "name": "Acme" } })).await?;
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "line", "data": { "customer": acme["data"]["id"], "qty": 3, "price": "19.99", "discount": "1.00" } }),
    )
    .await?;
    assert_eq!(created["data"]["customer_name"], "Acme");
    assert_eq!(created["data"]["subtotal"], "59.97");
    assert_eq!(created["data"]["total"], "58.97");
    // 58.97 * 0.075 = 4.42275, rounded half away from zero to 4.42.
    assert_eq!(created["data"]["vat"], "4.42");
    let id = created["data"]["id"].clone();

    // A change recalculates; a missing number counts as zero; the copy follows a cleared link.
    let updated = kernel_command(&ctx, "db::update", json!({ "model": "line", "id": id, "data": { "qty": 10, "discount": null, "customer": null } })).await?;
    assert_eq!(updated["data"]["subtotal"], "199.90");
    assert_eq!(updated["data"]["total"], "199.90");
    assert!(updated["data"].get("customer_name").is_none_or(Value::is_null));

    // A copy catches up with its source the next time the record is written.
    kernel_command(&ctx, "db::update", json!({ "model": "customer", "id": acme["data"]["id"], "data": { "name": "Acme Ltd" } })).await?;
    let relinked = kernel_command(&ctx, "db::update", json!({ "model": "line", "id": id, "data": { "customer": acme["data"]["id"] } })).await?;
    assert_eq!(relinked["data"]["customer_name"], "Acme Ltd");

    // Filters work on a calculated number, and plugins cannot write one.
    let found = kernel_command(&ctx, "db::find", json!({ "model": "line", "filter": { "total": { "gte": "100" } } })).await?;
    assert_eq!(found["data"].as_array().map(Vec::len), Some(1));
    let written = kernel_command(&ctx, "db::update", json!({ "model": "line", "id": id, "data": { "total": "1" } })).await;
    assert!(written.is_err(), "{written:?}");
    let written = kernel_command(&ctx, "db::create", json!({ "model": "line", "data": { "customer_name": "x" } })).await;
    assert!(written.is_err(), "{written:?}");
    Ok(())
}

fn invoice_with_lines() -> Result<Vec<ModelDef>, Box<dyn std::error::Error>> {
    let mut invoice: ModelDef = serde_json::from_value(json!({
        "name": "invoice",
        "fields": [
            { "name": "number", "type": "string", "sequence": { "pattern": "INV-{#####}" } },
            { "name": "lines", "type": "child", "target": "invoice_line", "inverse": "invoice", "order": "position" },
            { "name": "discount", "type": "decimal", "scale": 2 },
            { "name": "subtotal", "type": "decimal", "scale": 2, "compute": "sum(lines.amount)" },
            { "name": "total", "type": "decimal", "scale": 2, "compute": "subtotal - discount" },
            { "name": "line_count", "type": "int", "compute": "count(lines)" }
        ],
        "checks": [ { "require": { "total": { "gte": "0" } }, "message": "the total cannot be negative" } ]
    }))?;
    let mut line: ModelDef = serde_json::from_value(json!({
        "name": "invoice_line",
        "fields": [
            { "name": "invoice", "type": "link", "target": "invoice", "required": true },
            { "name": "position", "type": "int" },
            { "name": "item", "type": "string", "required": true },
            { "name": "qty", "type": "int", "min": 1 },
            { "name": "price", "type": "decimal", "scale": 2 },
            { "name": "amount", "type": "decimal", "scale": 2, "compute": "qty * price" }
        ]
    }))?;
    sync_ids(&mut invoice);
    sync_ids(&mut line);
    Ok(vec![invoice, line])
}

fn items(record: &Value) -> Vec<String> {
    record["lines"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["item"].as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn child_rows_are_written_read_totalled_and_deleted_with_their_record() -> TestResult {
    let Some(world) = World::new("children").await? else { return Ok(()) };
    let models = invoice_with_lines()?;
    for model in &models {
        assert!(model.problems().is_empty(), "{:?}", model.problems());
    }
    aether_core::data_model::definition::validate_set(&models)?;
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    // One call writes the record and its rows; the rows are numbered, the totals are worked out.
    let created = kernel_command(&ctx, "db::create", json!({
        "model": "invoice",
        "data": { "discount": "5", "lines": [
            { "item": "bolt", "qty": 10, "price": "1.50" },
            { "item": "nut", "qty": 4, "price": "0.25" },
            { "item": "washer", "qty": 2, "price": "10" }
        ] }
    })).await?;
    let id = created["data"]["id"].clone();
    assert_eq!(created["data"]["number"], "INV-00001");
    assert_eq!(items(&created["data"]), ["bolt", "nut", "washer"]);
    assert_eq!(created["data"]["lines"][1]["position"], 2);
    assert_eq!(created["data"]["lines"][0]["amount"], "15.00");
    assert_eq!(created["data"]["subtotal"], "36.00");
    assert_eq!(created["data"]["total"], "31.00");
    assert_eq!(created["data"]["line_count"], 3);
    // Every row points back at its record.
    assert_eq!(created["data"]["lines"][2]["invoice"], id);

    // Reading: rows only come with `expand`, in order.
    let plain = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert!(plain["data"].get("lines").is_none());
    let expanded = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id, "expand": ["lines"] })).await?;
    assert_eq!(items(&expanded["data"]), ["bolt", "nut", "washer"]);
    let listed = kernel_command(&ctx, "db::find", json!({ "model": "invoice", "expand": ["lines"] })).await?;
    assert_eq!(items(&listed["data"][0]), ["bolt", "nut", "washer"]);

    // An update makes the rows the list: keep with an id, add without, drop by leaving out.
    let lines = expanded["data"]["lines"].clone();
    let updated = kernel_command(&ctx, "db::update", json!({
        "model": "invoice", "id": id,
        "data": { "lines": [
            { "item": "gasket", "qty": 1, "price": "100" },
            { "id": lines[2]["id"], "item": "washer", "qty": 3, "price": "10" },
            { "id": lines[0]["id"], "item": "bolt", "qty": 10, "price": "1.50" }
        ] }
    })).await?;
    assert_eq!(items(&updated["data"]), ["gasket", "washer", "bolt"]);
    assert_eq!(updated["data"]["lines"][2]["position"], 3, "reordered");
    assert_eq!(updated["data"]["subtotal"], "145.00");
    assert_eq!(updated["data"]["line_count"], 3);
    assert_eq!(world.raw(&models[1].model_id.clone().unwrap_or_default()).await?.len(), 3, "the omitted row is gone");

    // A field left out of an update leaves the rows alone.
    let untouched = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "discount": "10" } })).await?;
    assert_eq!(untouched["data"]["total"], "135.00");
    let again = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id, "expand": ["lines"] })).await?;
    assert_eq!(again["data"]["lines"].as_array().map(Vec::len), Some(3));

    // A row written on its own brings its record's totals up to date.
    let first_row = again["data"]["lines"][0]["id"].clone();
    kernel_command(&ctx, "db::update", json!({ "model": "invoice_line", "id": first_row, "data": { "qty": 2 } })).await?;
    let after_edit = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_edit["data"]["subtotal"], "245.00");
    kernel_command(&ctx, "db::create", json!({ "model": "invoice_line", "data": { "invoice": id, "item": "extra", "qty": 1, "price": "5" } })).await?;
    let after_add = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_add["data"]["subtotal"], "250.00");
    assert_eq!(after_add["data"]["line_count"], 4);
    kernel_command(&ctx, "db::delete", json!({ "model": "invoice_line", "id": first_row })).await?;
    let after_delete = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_delete["data"]["line_count"], 3);

    // Refusals leave nothing behind: a bad row, a row of another record, a check on the totals.
    let bad_row = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "lines": [{ "item": "x", "qty": 0 }] } })).await;
    assert!(matches!(&bad_row, Err(_)), "{bad_row:?}");
    let other = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "lines": [{ "item": "solo", "qty": 1, "price": "1" }] } })).await?;
    let foreign = other["data"]["lines"][0]["id"].clone();
    let stolen = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "lines": [{ "id": foreign, "item": "solo" }] } })).await;
    assert!(matches!(&stolen, Err(HostError::InvalidPayload(m)) if m.contains("not a row of this record")), "{stolen:?}");
    let negative = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "discount": "9999" } })).await;
    assert!(matches!(&negative, Err(HostError::InvalidPayload(m)) if m.contains("cannot be negative")), "{negative:?}");
    let unchanged = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id, "expand": ["lines"] })).await?;
    assert_eq!(unchanged["data"]["lines"].as_array().map(Vec::len), Some(3), "a refused update changed no row");
    assert_eq!(unchanged["data"]["discount"], "10.00");

    // Deleting the record deletes its rows.
    kernel_command(&ctx, "db::delete", json!({ "model": "invoice", "id": id })).await?;
    let rows_left = world.raw(&models[1].model_id.clone().unwrap_or_default()).await?;
    assert_eq!(rows_left.len(), 1, "only the other invoice's row is left");
    assert!(kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?["data"].is_null());
    Ok(())
}

mod workflow_enforcement {
    use std::sync::Arc;

    use aether_core::data_model::RuleSet;

    use super::*;

    fn leave_model() -> Result<ModelDef, Box<dyn std::error::Error>> {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "leave",
            "chatter": { "enabled": true },
            "fields": [
                { "name": "employee", "type": "string", "required": true },
                { "name": "state", "type": "select", "default": "draft", "track": true,
                  "options": [{ "value": "draft" }, { "value": "submitted" }, { "value": "approved" }, { "value": "rejected" }] }
            ]
        }))?;
        sync_ids(&mut model);
        Ok(model)
    }

    fn rules() -> Result<RuleSet, serde_json::Error> {
        RuleSet::parse(
            &json!({
                "model": "leave",
                "access": [
                    { "name": "own", "operations": ["read", "create", "write"], "when": { "employee": "$user" } },
                    { "name": "approvers", "roles": ["approver"], "operations": ["read", "write"] }
                ],
                "workflow": {
                    "field": "state",
                    "transitions": [
                        { "name": "submit", "label": "Submit", "from": ["draft"], "to": "submitted", "when": { "employee": "$user" } },
                        { "name": "approve", "from": ["submitted"], "to": "approved", "roles": ["approver"], "when": { "employee": { "ne": "$user" } } },
                        { "name": "reject", "from": ["submitted"], "to": "rejected", "roles": ["approver"] },
                        { "name": "reopen", "from": ["rejected"], "to": "draft" }
                    ]
                }
            })
            .to_string(),
        )
    }

    async fn as_user(world: &World, model: &ModelDef, user: &str, roles: &[&str]) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
        let mut give = String::new();
        for role in roles {
            give.push_str(&format!(
                "UPSERT roles SET name = '{role}', label = '{role}' WHERE name = '{role}'; \
                 LET $u = (SELECT VALUE id FROM org_users WHERE core_user_id = '{user}' LIMIT 1)[0]; \
                 LET $r = (SELECT VALUE id FROM roles WHERE name = '{role}' LIMIT 1)[0]; \
                 UPSERT org_user_roles SET org_user = $u, role = $r WHERE org_user = $u AND role = $r;"
            ));
        }
        world
            .session
            .query(format!(
                "UPSERT org_users SET core_user_id = '{user}', display_name = '{user}', is_active = true WHERE core_user_id = '{user}'; {give}"
            ))
            .await?
            .check()?;
        let rules = Arc::new(rules()?);
        let grants: HashMap<String, ModelGrant> = schemas_of(std::slice::from_ref(model))
            .into_iter()
            .map(|(name, schema)| {
                let mut grant = ModelGrant::from_access(&name, &["read".into(), "write".into()], Some(&schema.table));
                grant.schema = Some(schema);
                grant.rules = Some(rules.clone());
                (name, grant)
            })
            .collect();
        Ok(PluginHostContext::new(
            "desk",
            ["db::query".to_string(), "db::mutate".to_string()].into_iter().collect(),
            grants,
            Arc::new(world.session.clone()),
            DbScope::new(NAMESPACE, &world.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext { actor: Actor::User(user.into()), request_id: "r".into(), ip: None, user_agent: None },
                "f",
            ),
        ))
    }

    fn names(reply: &Value) -> Vec<String> {
        reply["data"].as_array().map(|rows| rows.iter().filter_map(|r| r["name"].as_str().map(str::to_string)).collect()).unwrap_or_default()
    }

    async fn go(ctx: &PluginHostContext, id: &Value, to: &str) -> Result<Value, HostError> {
        kernel_command(ctx, "db::update", json!({ "model": "leave", "id": id, "data": { "state": to } })).await
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn records_move_only_along_transitions_the_caller_may_make() -> TestResult {
        let Some(world) = World::new("workflow").await? else { return Ok(()) };
        let model = leave_model()?;
        let problems = rules()?.problems(&model);
        assert!(problems.is_empty(), "{problems:?}");
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        let root = as_user(&world, &model, "users:root", &["org_admin"]).await?;

        // A new record starts as `draft`, whatever the caller asks for.
        let refused = kernel_command(&ann, "db::create", json!({ "model": "leave", "data": { "employee": "users:ann", "state": "approved" } })).await;
        assert!(matches!(&refused, Err(HostError::Denied(m)) if m.contains("starts as draft")), "{refused:?}");
        let made = kernel_command(&ann, "db::create", json!({ "model": "leave", "data": { "employee": "users:ann" } })).await?;
        let id = made["data"]["id"].clone();
        assert_eq!(made["data"]["state"], "draft");

        // The owner may submit, and sees exactly that button; nothing skips a step.
        assert_eq!(names(&kernel_command(&ann, "db::transitions", json!({ "model": "leave", "id": id })).await?), ["submit"]);
        let skipped = go(&ann, &id, "approved").await;
        assert!(matches!(&skipped, Err(HostError::Denied(_))), "{skipped:?}");
        assert_eq!(go(&ann, &id, "submitted").await?["data"]["state"], "submitted");
        let back = go(&ann, &id, "draft").await;
        assert!(matches!(&back, Err(HostError::Denied(m)) if m.contains("from its state now")), "{back:?}");
        // Writing the state it already has is not a move.
        assert_eq!(go(&ann, &id, "submitted").await?["data"]["state"], "submitted");
        assert!(names(&kernel_command(&ann, "db::transitions", json!({ "model": "leave", "id": id })).await?).is_empty());

        // An approver sees approve and reject, and approving is a move she may make.
        assert_eq!(names(&kernel_command(&carol, "db::transitions", json!({ "model": "leave", "id": id })).await?), ["approve", "reject"]);
        assert_eq!(go(&carol, &id, "approved").await?["data"]["state"], "approved");
        let reopen = go(&carol, &id, "draft").await;
        assert!(reopen.is_err(), "approved is final: {reopen:?}");

        // The workflow's own condition: nobody approves their own request.
        let mine = kernel_command(&carol, "db::create", json!({ "model": "leave", "data": { "employee": "users:carol" } })).await?;
        let mine_id = mine["data"]["id"].clone();
        go(&carol, &mine_id, "submitted").await?;
        assert_eq!(names(&kernel_command(&carol, "db::transitions", json!({ "model": "leave", "id": mine_id })).await?), ["reject"]);
        let own = go(&carol, &mine_id, "approved").await;
        assert!(matches!(&own, Err(HostError::Denied(_))), "{own:?}");

        // Rejected can be reopened by anyone who can change it; an administrator is not held to it.
        assert_eq!(go(&carol, &mine_id, "rejected").await?["data"]["state"], "rejected");
        assert_eq!(go(&carol, &mine_id, "draft").await?["data"]["state"], "draft");
        assert_eq!(go(&root, &id, "draft").await?["data"]["state"], "draft");

        // Every move was written in the record's history by the kernel.
        let lines = world.raw("chatter_messages").await?;
        assert!(lines.iter().filter(|line| line["kind"] == "change").count() >= 4, "{lines:?}");
        Ok(())
    }
}
