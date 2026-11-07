//! Models against a real SurrealDB (see `host_db_audit.rs` for how to run): applying them to an
//! organization, the kernel's commands through them, and what editing a model does.

use std::collections::HashMap;
use std::sync::Arc;

use aether_core::access::audit::{Actor, AuditContext};
use aether_core::data_model::{
    ModelDef,
    apply::{apply_plans, plan_models},
    schemas_of, sync_ids,
};
use aether_core::kernel::{
    CallInfo, DbScope, HostError, ModelGrant, PluginHostContext, kernel_command,
};
use aether_core::notifications::NotificationHub;
use serde_json::{Value, json};
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_data_models_test";

/// Databases are made one at a time within this test binary: two made at once in one namespace can
/// make SurrealDB refuse one with a write conflict, which has nothing to do with what is tested.
static SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        db.signin(Root {
            username: "root".into(),
            password: "root".into(),
        })
        .await?;
        let _one_at_a_time = SETUP.lock().await;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .subsec_nanos();
        let org = format!("org_{label}_{suffix}");
        aether_orm::migrate_org(&db, NAMESPACE, &org).await?;
        db.use_ns(NAMESPACE).use_db(&org).await?;
        Ok(Some(Self { session: db, org }))
    }

    /// Apply the models (the upgrade step) and say what it did.
    async fn apply(
        &self,
        models: &[ModelDef],
    ) -> Result<(), aether_core::data_model::apply::ApplyError> {
        let plans = plan_models(&self.session, "notes", models).await?;
        apply_plans(&self.session, &plans).await
    }

    /// A call by a plugin that holds these models, with write access.
    fn ctx(&self, models: &[ModelDef]) -> PluginHostContext {
        let grants: HashMap<String, ModelGrant> = schemas_of(models)
            .into_iter()
            .map(|(name, schema)| {
                let mut grant = ModelGrant::from_access(
                    &name,
                    &["read".into(), "write".into()],
                    Some(&schema.table),
                );
                grant.schema = Some(schema);
                (name, grant)
            })
            .collect();
        let mut ctx = PluginHostContext::new(
            "notes",
            ["db::query".to_string(), "db::mutate".to_string()]
                .into_iter()
                .collect(),
            grants,
            Arc::new(self.session.clone()),
            DbScope::new(NAMESPACE, &self.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext {
                    actor: Actor::User("users:u1".into()),
                    request_id: "r".into(),
                    ip: None,
                    user_agent: None,
                },
                "f",
            ),
        );
        // Every test here runs with Mantle attached, so reads are answered from it whenever they
        // can be and a stale answer would fail the assertion that follows the write.
        ctx.services = Some(aether_core::kernel::HostServices {
            cache: aether_core::cache::Cache::from_config(&aether_core::cache::CacheConfig {
                default_ttl_secs: None,
                coherence_signals: false,
                ..Default::default()
            })
            .expect("cache"),
            media: Arc::new(aether_storage::ObjectStoreBackend::new(
                object_store::memory::InMemory::new(),
            )),
            scheduler: None,
            files_root: None,
            bridges: None,
            core: None,
            public_base: String::new(),
            open_session: None,
        });
        ctx
    }

    async fn raw(&self, table: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let mut response = self
            .session
            .query(format!("SELECT * FROM {table} ORDER BY id;"))
            .await?
            .check()?;
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
    model
        .fields
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.id.clone())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn records_are_stored_under_field_ids_and_come_back_under_names() -> TestResult {
    let Some(world) = World::new("store").await? else {
        return Ok(());
    };
    let model = note();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));

    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "note", "data": { "title": "Plan", "pages": 3 } }),
    )
    .await?;
    let record = &created["data"];
    assert_eq!(record["title"], "Plan");
    assert_eq!(record["status"], "draft", "the default was applied");
    assert!(
        record["id"]
            .as_str()
            .is_some_and(|id| id.starts_with(model.model_id.as_deref().unwrap_or("?")))
    );

    // In the database only ids are used.
    let rows = world
        .raw(model.model_id.as_deref().unwrap_or_default())
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][id_of(&model, "title")], "Plan");
    assert!(
        rows[0].get("title").is_none(),
        "no field name is ever stored"
    );

    let found = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "note", "filter": { "status": "draft" }, "order": "title" }),
    )
    .await?;
    assert_eq!(found["data"][0]["title"], "Plan");
    let updated = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "note", "id": record["id"], "data": { "pages": 4, "body": "text" } }),
    )
    .await?;
    assert_eq!(updated["data"]["pages"], 4);
    // Null clears an optional field.
    let cleared = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "note", "id": record["id"], "data": { "body": null } }),
    )
    .await?;
    assert!(cleared["data"].get("body").is_none() || cleared["data"]["body"].is_null());
    let after = world
        .raw(model.model_id.as_deref().unwrap_or_default())
        .await?;
    assert!(
        after[0]
            .get(id_of(&model, "body"))
            .is_none_or(Value::is_null),
        "the value is gone from the record"
    );
    let gone = kernel_command(
        &ctx,
        "db::delete",
        json!({ "model": "note", "id": record["id"] }),
    )
    .await?;
    assert_eq!(gone["data"]["title"], "Plan");
    Ok(())
}

fn document(photo_type: &str) -> ModelDef {
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "document",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "photo", "type": photo_type },
            { "name": "scans", "type": "file", "multiple": true }
        ]
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    sync_ids(&mut model);
    model
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn image_and_file_fields_hold_uploads_and_keep_old_text() -> TestResult {
    let Some(world) = World::new("media").await? else {
        return Ok(());
    };
    // The field starts as text, with a data URL in it, as `hr.employee.image` did.
    let before = document("text");
    world.apply(std::slice::from_ref(&before)).await?;
    let ctx = world.ctx(std::slice::from_ref(&before));
    let legacy = "data:image/png;base64,AAAA";
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "document", "data": { "title": "Old", "photo": legacy } }),
    )
    .await?;
    let id = created["data"]["id"].clone();

    // It becomes an image: nothing is blocked and the old value is still there.
    let mut after = document("image");
    after.model_id = before.model_id.clone();
    for (new, old) in after.fields.iter_mut().zip(&before.fields) {
        new.id = old.id.clone();
    }
    world.apply(std::slice::from_ref(&after)).await?;
    let ctx = world.ctx(std::slice::from_ref(&after));
    let read = kernel_command(&ctx, "db::get", json!({ "model": "document", "id": id })).await?;
    assert_eq!(read["data"]["photo"], legacy);

    // The old text can be saved again; an upload replaces it.
    let upload = json!({
        "key": "media/2026/10/9f3c0a1b2c3d4e5f60718293a4b5c6d7.png",
        "name": "me.png", "mime": "image/png", "size": 1234
    });
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "document", "id": id, "data": { "title": "Same", "photo": legacy } }),
    )
    .await?;
    let replaced = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "document", "id": id, "data": { "photo": upload, "scans": [upload, upload] } }),
    )
    .await?;
    assert_eq!(replaced["data"]["photo"]["key"], upload["key"]);
    assert_eq!(replaced["data"]["scans"].as_array().map(Vec::len), Some(2));

    // What is not an upload is refused.
    for bad in [
        json!({ "photo": { "key": "plugins/hr/x.png", "name": "x", "mime": "image/png", "size": 1 } }),
        json!({ "photo": { "key": "media/a.pdf", "name": "x", "mime": "application/pdf", "size": 1 } }),
        json!({ "photo": 7 }),
        json!({ "scans": upload }),
        json!({ "scans": [7] }),
    ] {
        let result = kernel_command(
            &ctx,
            "db::update",
            json!({ "model": "document", "id": id, "data": bad.clone() }),
        )
        .await;
        assert!(
            matches!(result, Err(HostError::InvalidPayload(_) | HostError::Refused(_))),
            "{bad}: {result:?}"
        );
    }
    // Null clears.
    let cleared = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "document", "id": id, "data": { "photo": null } }),
    )
    .await?;
    assert!(cleared["data"].get("photo").is_none_or(Value::is_null));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn the_model_refuses_what_it_does_not_allow() -> TestResult {
    let Some(world) = World::new("rules").await? else {
        return Ok(());
    };
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
        assert!(
            matches!(result, Err(HostError::InvalidPayload(_) | HostError::Refused(_))),
            "{bad}: {result:?}"
        );
    }
    // The schema is also enforced by the database: a unique title cannot repeat.
    create(json!({ "title": "same" })).await?;
    assert!(
        create(json!({ "title": "same" })).await.is_err(),
        "the unique index refuses a repeat"
    );
    // Filters and ordering use names too, and refuse unknown ones.
    let filtered = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "note", "filter": { "nope": 1 } }),
    )
    .await;
    assert!(matches!(filtered, Err(HostError::InvalidPayload(_) | HostError::Refused(_))));
    let ordered = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "note", "order": "nope" }),
    )
    .await;
    assert!(matches!(ordered, Err(HostError::InvalidPayload(_) | HostError::Refused(_))));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn renaming_and_hiding_fields_and_adding_new_ones_moves_no_data() -> TestResult {
    let Some(world) = World::new("rename").await? else {
        return Ok(());
    };
    let before = note();
    world.apply(std::slice::from_ref(&before)).await?;
    let ctx = world.ctx(std::slice::from_ref(&before));
    for title in ["One", "Two", "Three"] {
        kernel_command(
            &ctx,
            "db::create",
            json!({ "model": "note", "data": { "title": title, "body": "b", "pages": 1 } }),
        )
        .await?;
    }
    let table = before.model_id.clone().unwrap_or_default();
    let stored_before = world.raw(&table).await?;

    // Rename `title`, hide `pages`, add `tags`, relabel the model: all at once.
    let mut after = before.clone();
    after.fields[0].name = "heading".into();
    after.fields[0].label = Some("Heading".into());
    after.fields[3].deprecated = true;
    after.label = Some("Renamed".into());
    after.fields.push(serde_json::from_value(
        json!({ "name": "tags", "type": "string" }),
    )?);
    sync_ids(&mut after);
    let plans = plan_models(&world.session, "notes", std::slice::from_ref(&after)).await?;
    assert!(plans[0].0.blockers.is_empty());
    assert!(
        plans[0]
            .0
            .ops
            .iter()
            .all(|op| !matches!(op, aether_core::data_model::apply::Op::Backfill { .. })),
        "no record is rewritten"
    );
    apply_plans(&world.session, &plans).await?;

    // Every stored record is exactly as it was.
    assert_eq!(
        world.raw(&table).await?,
        stored_before,
        "nothing was copied, renamed or rewritten"
    );

    // Through the new definition the same records read under the new names; the hidden field
    // is gone from the plugin's point of view but its data is still there.
    let ctx = world.ctx(std::slice::from_ref(&after));
    let found = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "note", "order": "heading" }),
    )
    .await?;
    let rows = found["data"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        rows.iter()
            .map(|r| r["heading"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        ["One", "Three", "Two"]
    );
    assert!(
        rows.iter()
            .all(|row| row.get("title").is_none() && row.get("pages").is_none())
    );
    assert!(
        world
            .raw(&table)
            .await?
            .iter()
            .all(|row| row.get(id_of(&before, "pages")).is_some()),
        "hidden data is kept"
    );
    assert!(matches!(
        kernel_command(
            &ctx,
            "db::create",
            json!({ "model": "note", "data": { "title": "old name" } })
        )
        .await,
        Err(HostError::InvalidPayload(_) | HostError::Refused(_))
    ));
    kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "note", "data": { "heading": "Four", "tags": "a" } }),
    )
    .await?;

    // Applying the same thing again is a no-op.
    let again = plan_models(&world.session, "notes", std::slice::from_ref(&after)).await?;
    assert!(
        again[0].0.is_noop(again[0].1.as_ref()),
        "{:?}",
        again[0].0.ops
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn unsafe_changes_are_blocked_before_anything_is_touched() -> TestResult {
    let Some(world) = World::new("blocked").await? else {
        return Ok(());
    };
    let before = note();
    world.apply(std::slice::from_ref(&before)).await?;
    let ctx = world.ctx(std::slice::from_ref(&before));
    kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "note", "data": { "title": "One", "pages": 2 } }),
    )
    .await?;
    let table = before.model_id.clone().unwrap_or_default();
    let stored = world.raw(&table).await?;

    // A new required field with no default, over existing records.
    let mut required = before.clone();
    required.fields.push(serde_json::from_value(
        json!({ "name": "owner", "type": "string", "required": true }),
    )?);
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
    assert_eq!(
        world.raw(&table).await?,
        stored,
        "a blocked upgrade changes nothing"
    );

    // With a default the required field is allowed, and existing records get it.
    required.fields[4].default = Some(json!("nobody"));
    world.apply(std::slice::from_ref(&required)).await?;
    let owner = id_of(&required, "owner");
    assert!(
        world
            .raw(&table)
            .await?
            .iter()
            .all(|row| row[owner.as_str()] == "nobody"),
        "records were backfilled"
    );
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
    let Some(world) = World::new("chatter").await? else {
        return Ok(());
    };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let table = model.model_id.clone().unwrap_or_default();

    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ticket", "data": { "title": "Fix" } }),
    )
    .await?;
    let id = created["data"]["id"].clone();
    let lines = world.raw("chatter_messages").await?;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "system");
    assert_eq!(lines[0]["author"], "users:u1");
    assert_eq!(lines[0]["model_id"], table.as_str());
    let key = lines[0]["record_key"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        id.as_str().is_some_and(|id| id.ends_with(&key)),
        "the thread is keyed by the record's key"
    );
    assert_eq!(
        world.raw("chatter_followers").await?.len(),
        1,
        "the creator follows"
    );

    // An untracked field changes nothing in the thread; a tracked one is recorded as before/after.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ticket", "id": id, "data": { "notes": "x" } }),
    )
    .await?;
    assert_eq!(world.raw("chatter_messages").await?.len(), 1);
    kernel_command(&ctx, "db::update", json!({ "model": "ticket", "id": id, "data": { "status": "closed", "points": 3, "notes": "y" } })).await?;
    let lines = world.raw("chatter_messages").await?;
    assert_eq!(lines.len(), 2);
    // Record ids are random, so the rows come back in no particular order.
    let change = lines
        .iter()
        .find(|line| line["kind"] == "change")
        .ok_or("no change line")?;
    assert_eq!(change["before"][id_of(&model, "status")], "open");
    assert_eq!(change["after"][id_of(&model, "status")], "closed");
    assert_eq!(change["after"][id_of(&model, "points")], 3);
    assert!(
        change["after"].get(id_of(&model, "notes")).is_none(),
        "only tracked fields"
    );

    // Writing the same value again is not a change.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ticket", "id": id, "data": { "status": "closed" } }),
    )
    .await?;
    assert_eq!(world.raw("chatter_messages").await?.len(), 2);

    kernel_command(&ctx, "db::delete", json!({ "model": "ticket", "id": id })).await?;
    let lines = world.raw("chatter_messages").await?;
    assert!(
        lines
            .iter()
            .all(|line| line["record_deleted_at"].is_string()),
        "the thread waits in the trash"
    );
    assert!(world.raw("chatter_followers").await?.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_model_without_chatter_writes_nothing_to_it() -> TestResult {
    let Some(world) = World::new("nochatter").await? else {
        return Ok(());
    };
    let model = note();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "note", "data": { "title": "A" } }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "note", "id": created["data"]["id"], "data": { "pages": 2 } }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::delete",
        json!({ "model": "note", "id": created["data"]["id"] }),
    )
    .await?;
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
    script_plugin_uses_the_same_models_grants_and_chatter(
        "rhai",
        aether_core::plugin_manager::script::ScriptKind::Rhai,
        SCRIPT,
    )
    .await
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_lua_plugin_uses_the_same_models_grants_and_chatter() -> TestResult {
    script_plugin_uses_the_same_models_grants_and_chatter(
        "lua",
        aether_core::plugin_manager::script::ScriptKind::Lua,
        LUA_SCRIPT,
    )
    .await
}

async fn script_plugin_uses_the_same_models_grants_and_chatter(
    label: &str,
    kind: aether_core::plugin_manager::script::ScriptKind,
    source: &str,
) -> TestResult {
    use aether_core::plugin_manager::script::{ScriptError, ScriptProgram};
    let Some(world) = World::new(label).await? else {
        return Ok(());
    };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let program = Arc::new(ScriptProgram::compile(kind, source)?);

    let created = run(&program, &ctx, "add", json!({ "title": "From a script" })).await?;
    assert_eq!(created["title"], "From a script");
    assert_eq!(created["status"], "open", "defaults apply");
    assert_eq!(
        run(&program, &ctx, "titles", Value::Null).await?,
        json!(["From a script"])
    );
    assert_eq!(
        run(&program, &ctx, "who", Value::Null).await?["id"],
        "users:u1"
    );

    // The model still refuses what it does not allow, and the failure is internal.
    assert!(matches!(
        run(&program, &ctx, "bad", Value::Null).await,
        Err(ScriptError::Failed(_))
    ));

    // `fail` reaches the caller; a change made by a script is tracked like any other.
    let closed = run(&program, &ctx, "close", json!({ "id": created["id"] })).await?;
    assert_eq!(closed["status"], "closed");
    let lines = world.raw("chatter_messages").await?;
    assert!(lines.iter().any(|line| line["kind"] == "change"));
    let missing = run(&program, &ctx, "close", json!({ "id": "nope" })).await;
    assert!(
        matches!(missing, Err(ScriptError::User(ref message)) if message == "no such ticket"),
        "{missing:?}"
    );

    // Without the capability the script is refused, as a WASM plugin would be.
    let mut read_only = ctx.clone();
    read_only.granted_capabilities.remove("db::mutate");
    assert!(matches!(
        run(&program, &read_only, "add", json!({ "title": "x" })).await,
        Err(ScriptError::Failed(_))
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_unique_multi_field_index_refuses_duplicates_and_follows_renames() -> TestResult {
    let Some(world) = World::new("composite").await? else {
        return Ok(());
    };
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
    let again = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "assignment", "data": { "employee": "bob", "post": "clerk" } }),
    )
    .await;
    assert!(matches!(again, Err(HostError::Db(_))), "{again:?}");

    // Dropping the index lets the duplicate in.
    model.indexes.clear();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "assignment", "data": { "employee": "bob", "post": "clerk" } }),
    )
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn unique_when_allows_one_matching_record_per_group() -> TestResult {
    let Some(world) = World::new("unique_when").await? else {
        return Ok(());
    };
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "supplier_row",
        "fields": [
            { "name": "item", "type": "string" },
            { "name": "preferred", "type": "bool", "default": false }
        ],
        "unique_when": [ {
            "fields": ["item"], "when": { "preferred": true },
            "message": "one preferred row per item"
        } ]
    }))?;
    sync_ids(&mut model);
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let make = |item: &str, preferred: bool| {
        json!({ "model": "supplier_row", "data": { "item": item, "preferred": preferred } })
    };
    kernel_command(&ctx, "db::create", make("a", true)).await?;
    kernel_command(&ctx, "db::create", make("a", false)).await?;
    kernel_command(&ctx, "db::create", make("a", false)).await?;
    kernel_command(&ctx, "db::create", make("b", true)).await?;
    let second = kernel_command(&ctx, "db::create", make("a", true)).await;
    assert!(second.is_err(), "{second:?}");
    // Promoting another row of the group is refused too, until the first is cleared.
    let rows = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "supplier_row", "filter": { "item": "a", "preferred": false } }),
    )
    .await?;
    let id = rows["data"][0]["id"].as_str().unwrap_or_default().to_string();
    let promote = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "supplier_row", "id": id, "data": { "preferred": true } }),
    )
    .await;
    assert!(promote.is_err(), "{promote:?}");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_series_numbers_on_a_state_change_and_per_scope() -> TestResult {
    let Some(world) = World::new("series_on").await? else {
        return Ok(());
    };
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "order",
        "fields": [
            { "name": "company", "type": "string" },
            { "name": "state", "type": "select", "default": "draft",
              "options": [ { "value": "draft" }, { "value": "confirmed" } ] },
            { "name": "number", "type": "string",
              "sequence": { "pattern": "SO-{#####}", "scope": "company",
                            "assign_on": { "field": "state", "values": ["confirmed"] } } }
        ]
    }))?;
    sync_ids(&mut model);
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let make = |company: &str| json!({ "model": "order", "data": { "company": company } });
    let a1 = kernel_command(&ctx, "db::create", make("a")).await?;
    let a2 = kernel_command(&ctx, "db::create", make("a")).await?;
    let b1 = kernel_command(&ctx, "db::create", make("b")).await?;
    // A draft has no number.
    assert!(a1["data"]["number"].is_null(), "{a1:?}");
    let confirm = |id: &Value| json!({ "model": "order", "id": id["data"]["id"], "data": { "state": "confirmed" } });
    let second = kernel_command(&ctx, "db::update", confirm(&a2)).await?;
    let first = kernel_command(&ctx, "db::update", confirm(&a1)).await?;
    let other = kernel_command(&ctx, "db::update", confirm(&b1)).await?;
    // Numbers follow confirmation, and each company counts for itself.
    assert_eq!(second["data"]["number"], "SO-00001");
    assert_eq!(first["data"]["number"], "SO-00002");
    assert_eq!(other["data"]["number"], "SO-00001");
    // Writing the state again keeps the number and takes no new one.
    let again = kernel_command(&ctx, "db::update", confirm(&a2)).await?;
    assert_eq!(again["data"]["number"], "SO-00001");
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
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_hierarchy_keeps_its_edges_refuses_loops_and_walks_any_depth() -> TestResult {
    let Some(world) = World::new("tree").await? else {
        return Ok(());
    };
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
    let ministry = kernel_command(&ctx, "db::create", make("ministry", None)).await?["data"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let dept =
        kernel_command(&ctx, "db::create", make("dept", Some(&ministry))).await?["data"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
    let unit = kernel_command(&ctx, "db::create", make("unit", Some(&dept))).await?["data"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let other =
        kernel_command(&ctx, "db::create", make("other", Some(&ministry))).await?["data"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();

    let tree = |id: &str, direction: &str, depth: Option<u32>, include_self: bool| {
        let mut payload = json!({ "model": "unit", "field": "parent", "id": id, "direction": direction, "include_self": include_self });
        if let Some(depth) = depth {
            payload["depth"] = json!(depth);
        }
        kernel_command(&ctx, "db::tree", payload)
    };
    assert_eq!(
        names(&tree(&ministry, "down", None, false).await?),
        ["dept", "other", "unit"]
    );
    assert_eq!(
        names(&tree(&ministry, "down", Some(1), false).await?),
        ["dept", "other"]
    );
    assert_eq!(
        names(&tree(&ministry, "down", None, true).await?),
        ["dept", "ministry", "other", "unit"]
    );
    assert_eq!(
        names(&tree(&unit, "up", None, false).await?),
        ["dept", "ministry"]
    );
    assert!(names(&tree(&other, "down", None, false).await?).is_empty());

    // A parent that does not exist, a loop, and a record under itself are all refused...
    let missing = kernel_command(
        &ctx,
        "db::create",
        make(
            "ghost",
            Some(&format!(
                "{}:nothing",
                ministry.split(':').next().unwrap_or_default()
            )),
        ),
    )
    .await;
    assert!(
        matches!(missing, Err(HostError::InvalidPayload(ref m)) if m.contains("does not exist")),
        "{missing:?}"
    );
    for (record, parent) in [(&ministry, &unit), (&ministry, &ministry), (&dept, &unit)] {
        let looped = kernel_command(
            &ctx,
            "db::update",
            json!({ "model": "unit", "id": record, "data": { "parent": parent } }),
        )
        .await;
        assert!(
            matches!(looped, Err(HostError::InvalidPayload(ref m)) if m.contains("loop")),
            "{looped:?}"
        );
    }
    // ...and a failed move changes nothing.
    assert_eq!(
        names(&tree(&ministry, "down", None, false).await?),
        ["dept", "other", "unit"]
    );

    // Moving a subtree is one write; the walk follows at once.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "unit", "id": dept, "data": { "parent": other } }),
    )
    .await?;
    assert_eq!(
        names(&tree(&other, "down", None, false).await?),
        ["dept", "unit"]
    );
    assert_eq!(
        names(&tree(&unit, "up", None, false).await?),
        ["dept", "ministry", "other"]
    );
    // Clearing the parent makes a root.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "unit", "id": dept, "data": { "parent": null } }),
    )
    .await?;
    assert!(names(&tree(&other, "down", None, false).await?).is_empty());
    assert_eq!(names(&tree(&dept, "down", None, false).await?), ["unit"]);

    // A record with children is not deleted; a leaf is.
    let blocked = kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": dept })).await;
    assert!(
        matches!(blocked, Err(HostError::InvalidPayload(ref m)) if m.contains("children")),
        "{blocked:?}"
    );
    kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": unit })).await?;
    kernel_command(&ctx, "db::delete", json!({ "model": "unit", "id": dept })).await?;
    assert_eq!(
        names(&tree(&ministry, "down", None, false).await?),
        ["other"]
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn many_to_many_fields_are_edges_read_both_ways() -> TestResult {
    let Some(world) = World::new("m2m").await? else {
        return Ok(());
    };
    let models = org_models();
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    let id = |reply: Value| reply["data"]["id"].as_str().unwrap_or_default().to_string();
    let unit = id(kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "unit", "data": { "name": "u" } }),
    )
    .await?);
    let other = id(kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "unit", "data": { "name": "o" } }),
    )
    .await?);
    let rust = id(kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "skill", "data": { "name": "rust" } }),
    )
    .await?);
    let go = id(kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "skill", "data": { "name": "go" } }),
    )
    .await?);

    kernel_command(
        &ctx,
        "db::relate",
        json!({ "model": "unit", "field": "skills", "id": unit, "to": [rust, go] }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::relate",
        json!({ "model": "unit", "field": "skills", "id": other, "to": [rust] }),
    )
    .await?;
    // Again is harmless.
    kernel_command(
        &ctx,
        "db::relate",
        json!({ "model": "unit", "field": "skills", "id": unit, "to": [rust] }),
    )
    .await?;

    let skills = kernel_command(
        &ctx,
        "db::related",
        json!({ "model": "unit", "field": "skills", "id": unit }),
    )
    .await?;
    assert_eq!(names(&skills), ["go", "rust"]);
    let who = kernel_command(
        &ctx,
        "db::related",
        json!({ "model": "unit", "field": "skills", "id": rust, "reverse": true }),
    )
    .await?;
    assert_eq!(names(&who), ["o", "u"]);

    kernel_command(
        &ctx,
        "db::unrelate",
        json!({ "model": "unit", "field": "skills", "id": unit, "to": [go] }),
    )
    .await?;
    let skills = kernel_command(
        &ctx,
        "db::related",
        json!({ "model": "unit", "field": "skills", "id": unit }),
    )
    .await?;
    assert_eq!(names(&skills), ["rust"]);

    // Wrong target model, a record that is not there, a field that is not a relation: all refused.
    let wrong = kernel_command(
        &ctx,
        "db::relate",
        json!({ "model": "unit", "field": "skills", "id": unit, "to": [other] }),
    )
    .await;
    assert!(
        matches!(wrong, Err(HostError::InvalidPayload(_) | HostError::Refused(_))),
        "{wrong:?}"
    );
    let table = rust.split(':').next().unwrap_or_default();
    let ghost = kernel_command(&ctx, "db::relate", json!({ "model": "unit", "field": "skills", "id": unit, "to": [format!("{table}:nothing")] })).await;
    assert!(
        matches!(ghost, Err(HostError::InvalidPayload(ref m)) if m.contains("does not exist")),
        "{ghost:?}"
    );
    let not_relation = kernel_command(
        &ctx,
        "db::relate",
        json!({ "model": "unit", "field": "name", "id": unit, "to": [rust] }),
    )
    .await;
    assert!(
        matches!(not_relation, Err(HostError::InvalidPayload(_) | HostError::Refused(_))),
        "{not_relation:?}"
    );
    // A many2many field is not a value either.
    let as_value = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "unit", "id": unit, "data": { "skills": [rust] } }),
    )
    .await;
    assert!(
        matches!(as_value, Err(HostError::InvalidPayload(_) | HostError::Refused(_))),
        "{as_value:?}"
    );

    // Deleting a record takes its edges with it.
    kernel_command(&ctx, "db::delete", json!({ "model": "skill", "id": rust })).await?;
    let skills = kernel_command(
        &ctx,
        "db::related",
        json!({ "model": "unit", "field": "skills", "id": unit }),
    )
    .await?;
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
        async fn call(
            &self,
            _plugin: &str,
            function: &str,
            _payload: Value,
            _trail: Vec<String>,
        ) -> Result<Value, HostError> {
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

    async fn as_user(
        world: &World,
        model: &ModelDef,
        user: &str,
        roles: &[&str],
    ) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
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
                let mut grant = ModelGrant::from_access(
                    &name,
                    &["read".into(), "write".into()],
                    Some(&schema.table),
                );
                grant.schema = Some(schema);
                grant.rules = Some(Arc::new(rules()));
                (name, grant)
            })
            .collect();
        Ok(PluginHostContext::new(
            "desk",
            ["db::query".to_string(), "db::mutate".to_string()]
                .into_iter()
                .collect(),
            grants,
            Arc::new(world.session.clone()),
            DbScope::new(NAMESPACE, &world.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext {
                    actor: Actor::User(user.into()),
                    request_id: "r".into(),
                    ip: None,
                    user_agent: None,
                },
                "f",
            ),
        )
        .with_plugin_calls(Vec::new(), Vec::new(), Arc::new(Variables)))
    }

    fn titles(reply: &Value) -> Vec<String> {
        let mut found: Vec<String> = reply["data"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r["title"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        found
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn rules_decide_who_sees_and_changes_what() -> TestResult {
        let Some(world) = World::new("rules").await? else {
            return Ok(());
        };
        let model = ticket_model();
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &["desk.lead"]).await?;
        let bob = as_user(&world, &model, "users:bob", &[]).await?;
        let cy = as_user(&world, &model, "users:cy", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        let root = as_user(&world, &model, "users:root", &["org_admin"]).await?;

        async fn make(
            ctx: &PluginHostContext,
            owner: &str,
            title: &str,
        ) -> Result<Value, HostError> {
            kernel_command(ctx, "db::create", json!({ "model": "ticket", "data": { "owner": owner, "title": title, "notes": "private" } })).await
        }
        // Everyone can create their own; nobody can create someone else's.
        let ann_ticket = make(&ann, "users:ann", "ann-1").await?["data"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let bob_ticket = make(&bob, "users:bob", "bob-1").await?["data"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        make(&cy, "users:cy", "cy-1").await?;
        let forged = make(&bob, "users:cy", "forged").await;
        assert!(matches!(forged, Err(HostError::Denied(_))), "{forged:?}");
        assert_eq!(
            titles(&kernel_command(&root, "db::find", json!({ "model": "ticket" })).await?).len(),
            3,
            "the refused create left nothing"
        );

        // Reading: your own; a lead also sees the team (the variable); an approver sees all; an admin all.
        let all = json!({ "model": "ticket" });
        assert_eq!(
            titles(&kernel_command(&bob, "db::find", all.clone()).await?),
            ["bob-1"]
        );
        assert_eq!(
            titles(&kernel_command(&ann, "db::find", all.clone()).await?),
            ["ann-1", "bob-1"]
        );
        assert_eq!(
            titles(&kernel_command(&carol, "db::find", all.clone()).await?),
            ["ann-1", "bob-1", "cy-1"]
        );
        assert_eq!(
            titles(&kernel_command(&root, "db::find", all.clone()).await?).len(),
            3
        );
        assert_eq!(
            kernel_command(&bob, "db::count", all.clone()).await?["data"],
            1
        );
        assert_eq!(
            kernel_command(&carol, "db::count", all.clone()).await?["data"],
            3
        );
        // A record you may not read is as if it were not there.
        let cy_ticket = kernel_command(
            &root,
            "db::find",
            json!({ "model": "ticket", "filter": { "owner": "users:cy" } }),
        )
        .await?["data"][0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(
            kernel_command(
                &bob,
                "db::get",
                json!({ "model": "ticket", "id": cy_ticket })
            )
            .await?["data"]
                .is_null()
        );
        assert_eq!(
            kernel_command(
                &bob,
                "db::get",
                json!({ "model": "ticket", "id": bob_ticket })
            )
            .await?["data"]["title"],
            "bob-1"
        );

        // Fields: notes are for approvers only; the state is theirs to set.
        let mine = kernel_command(
            &bob,
            "db::get",
            json!({ "model": "ticket", "id": bob_ticket }),
        )
        .await?;
        assert!(mine["data"].get("notes").is_none(), "hidden field: {mine}");
        let approver_view = kernel_command(
            &carol,
            "db::get",
            json!({ "model": "ticket", "id": bob_ticket }),
        )
        .await?;
        assert_eq!(approver_view["data"]["notes"], "private");
        let search_hidden = kernel_command(
            &bob,
            "db::find",
            json!({ "model": "ticket", "filter": { "notes": "private" } }),
        )
        .await;
        assert!(
            matches!(search_hidden, Err(HostError::Denied(_))),
            "{search_hidden:?}"
        );
        let self_approve = kernel_command(
            &bob,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "state": "approved" } }),
        )
        .await;
        assert!(
            matches!(self_approve, Err(HostError::Denied(_))),
            "{self_approve:?}"
        );

        // Writing: your own while pending; never someone else's; approvers any, any time.
        kernel_command(
            &bob,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "bob-1b" } }),
        )
        .await?;
        let others = kernel_command(
            &bob,
            "db::update",
            json!({ "model": "ticket", "id": cy_ticket, "data": { "title": "x" } }),
        )
        .await;
        assert!(matches!(others, Err(HostError::Denied(_))), "{others:?}");
        let lead_write = kernel_command(
            &ann,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "x" } }),
        )
        .await;
        assert!(
            matches!(lead_write, Err(HostError::Denied(_))),
            "a lead may read the team, not change it: {lead_write:?}"
        );
        kernel_command(
            &carol,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "state": "approved" } }),
        )
        .await?;
        // Once approved, the owner can no longer change or delete it.
        let after = kernel_command(
            &bob,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "late" } }),
        )
        .await;
        assert!(matches!(after, Err(HostError::Denied(_))), "{after:?}");
        let removed = kernel_command(
            &bob,
            "db::delete",
            json!({ "model": "ticket", "id": bob_ticket }),
        )
        .await;
        assert!(matches!(removed, Err(HostError::Denied(_))), "{removed:?}");
        // ...but a pending one of their own can be deleted, and an approver can still change the approved one.
        kernel_command(
            &cy,
            "db::delete",
            json!({ "model": "ticket", "id": cy_ticket }),
        )
        .await?;
        kernel_command(
            &carol,
            "db::update",
            json!({ "model": "ticket", "id": bob_ticket, "data": { "title": "approved-1" } }),
        )
        .await?;
        assert_eq!(
            titles(&kernel_command(&root, "db::find", all).await?),
            ["ann-1", "approved-1"]
        );
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
    let Some(world) = World::new("integrity").await? else {
        return Ok(());
    };
    let models = invoice_models()?;
    world.apply(&models).await?;
    let ctx = world.ctx(&models);
    let year = chrono::Datelike::year(&chrono::Utc::now());

    let customer = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "customer", "data": { "name": "Acme" } }),
    )
    .await?;
    let customer_id = customer["data"]["id"].clone();

    // Numbers come from the series, in order, and the created record shows its number.
    let first = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "customer": customer_id, "amount": "10.00" } }),
    )
    .await?;
    assert_eq!(first["data"]["number"], format!("INV-{year}-001"));
    let second = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "amount": "20" } }),
    )
    .await?;
    assert_eq!(second["data"]["number"], format!("INV-{year}-002"));

    // A create that fails gives its number back: nothing is skipped.
    let refused = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "status": "paid" } }),
    )
    .await;
    assert!(
        matches!(&refused, Err(HostError::InvalidPayload(m)) if m.contains("needs the date it was paid")),
        "{refused:?}"
    );
    let third = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "amount": "30" } }),
    )
    .await?;
    assert_eq!(
        third["data"]["number"],
        format!("INV-{year}-003"),
        "the refused create used no number"
    );

    // A number given by the caller is kept and takes nothing from the series.
    let given = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "number": "MANUAL-1" } }),
    )
    .await?;
    assert_eq!(given["data"]["number"], "MANUAL-1");
    let fourth = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "amount": "1" } }),
    )
    .await?;
    assert_eq!(fourth["data"]["number"], format!("INV-{year}-004"));

    // Limits.
    let low = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "amount": "0" } }),
    )
    .await;
    assert!(low.is_err(), "{low:?}");
    let high = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": first["data"]["id"], "data": { "amount": "1000.01" } }),
    )
    .await;
    assert!(high.is_err(), "{high:?}");

    // Checks on update, including one field against another.
    let id = first["data"]["id"].clone();
    let paid = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "status": "paid" } }),
    )
    .await;
    assert!(
        matches!(&paid, Err(HostError::InvalidPayload(m)) if m.contains("needs the date")),
        "{paid:?}"
    );
    kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "status": "paid", "paid_on": "2026-10-01" } })).await?;
    let backwards = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "start": "2026-10-05", "end": "2026-10-01" } })).await;
    assert!(
        matches!(&backwards, Err(HostError::InvalidPayload(m)) if m.contains("end is before the start")),
        "{backwards:?}"
    );
    kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "start": "2026-10-01", "end": "2026-10-05" } })).await?;
    let after = kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(
        after["data"]["end"], "2026-10-05",
        "a refused write left nothing behind"
    );

    // A link must point at a record that exists.
    let ghost = format!(
        "{}:doesnotexist",
        models[0].model_id.clone().unwrap_or_default()
    );
    let dangling = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "invoice", "data": { "customer": ghost } }),
    )
    .await;
    assert!(
        matches!(&dangling, Err(HostError::InvalidPayload(m)) if m.contains("does not exist")),
        "{dangling:?}"
    );
    let dangling = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "customer": ghost } }),
    )
    .await;
    assert!(dangling.is_err(), "{dangling:?}");
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "customer": customer_id } }),
    )
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn calculated_fields_are_filled_exactly_on_every_write_and_cannot_be_written() -> TestResult {
    let Some(world) = World::new("derived").await? else {
        return Ok(());
    };
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

    let acme = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "customer", "data": { "name": "Acme" } }),
    )
    .await?;
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
    assert!(
        updated["data"]
            .get("customer_name")
            .is_none_or(Value::is_null)
    );

    // A copy catches up with its source the next time the record is written.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "customer", "id": acme["data"]["id"], "data": { "name": "Acme Ltd" } }),
    )
    .await?;
    let relinked = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "line", "id": id, "data": { "customer": acme["data"]["id"] } }),
    )
    .await?;
    assert_eq!(relinked["data"]["customer_name"], "Acme Ltd");

    // Filters work on a calculated number, and plugins cannot write one.
    let found = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "line", "filter": { "total": { "gte": "100" } } }),
    )
    .await?;
    assert_eq!(found["data"].as_array().map(Vec::len), Some(1));
    let written = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "line", "id": id, "data": { "total": "1" } }),
    )
    .await;
    assert!(written.is_err(), "{written:?}");
    let written = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "line", "data": { "customer_name": "x" } }),
    )
    .await;
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
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["item"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn child_rows_are_written_read_totalled_and_deleted_with_their_record() -> TestResult {
    let Some(world) = World::new("children").await? else {
        return Ok(());
    };
    let models = invoice_with_lines()?;
    for model in &models {
        assert!(model.problems().is_empty(), "{:?}", model.problems());
    }
    aether_core::data_model::definition::validate_set(&models)?;
    world.apply(&models).await?;
    let ctx = world.ctx(&models);

    // One call writes the record and its rows; the rows are numbered, the totals are worked out.
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({
            "model": "invoice",
            "data": { "discount": "5", "lines": [
                { "item": "bolt", "qty": 10, "price": "1.50" },
                { "item": "nut", "qty": 4, "price": "0.25" },
                { "item": "washer", "qty": 2, "price": "10" }
            ] }
        }),
    )
    .await?;
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
    let expanded = kernel_command(
        &ctx,
        "db::get",
        json!({ "model": "invoice", "id": id, "expand": ["lines"] }),
    )
    .await?;
    assert_eq!(items(&expanded["data"]), ["bolt", "nut", "washer"]);
    let listed = kernel_command(
        &ctx,
        "db::find",
        json!({ "model": "invoice", "expand": ["lines"] }),
    )
    .await?;
    assert_eq!(items(&listed["data"][0]), ["bolt", "nut", "washer"]);

    // An update makes the rows the list: keep with an id, add without, drop by leaving out.
    let lines = expanded["data"]["lines"].clone();
    let updated = kernel_command(
        &ctx,
        "db::update",
        json!({
            "model": "invoice", "id": id,
            "data": { "lines": [
                { "item": "gasket", "qty": 1, "price": "100" },
                { "id": lines[2]["id"], "item": "washer", "qty": 3, "price": "10" },
                { "id": lines[0]["id"], "item": "bolt", "qty": 10, "price": "1.50" }
            ] }
        }),
    )
    .await?;
    assert_eq!(items(&updated["data"]), ["gasket", "washer", "bolt"]);
    assert_eq!(updated["data"]["lines"][2]["position"], 3, "reordered");
    assert_eq!(updated["data"]["subtotal"], "145.00");
    assert_eq!(updated["data"]["line_count"], 3);
    assert_eq!(
        world
            .raw(&models[1].model_id.clone().unwrap_or_default())
            .await?
            .len(),
        3,
        "the omitted row is gone"
    );

    // A field left out of an update leaves the rows alone.
    let untouched = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "discount": "10" } }),
    )
    .await?;
    assert_eq!(untouched["data"]["total"], "135.00");
    let again = kernel_command(
        &ctx,
        "db::get",
        json!({ "model": "invoice", "id": id, "expand": ["lines"] }),
    )
    .await?;
    assert_eq!(again["data"]["lines"].as_array().map(Vec::len), Some(3));

    // A row written on its own brings its record's totals up to date.
    let first_row = again["data"]["lines"][0]["id"].clone();
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice_line", "id": first_row, "data": { "qty": 2 } }),
    )
    .await?;
    let after_edit =
        kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_edit["data"]["subtotal"], "245.00");
    kernel_command(&ctx, "db::create", json!({ "model": "invoice_line", "data": { "invoice": id, "item": "extra", "qty": 1, "price": "5" } })).await?;
    let after_add =
        kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_add["data"]["subtotal"], "250.00");
    assert_eq!(after_add["data"]["line_count"], 4);
    kernel_command(
        &ctx,
        "db::delete",
        json!({ "model": "invoice_line", "id": first_row }),
    )
    .await?;
    let after_delete =
        kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?;
    assert_eq!(after_delete["data"]["line_count"], 3);

    // Refusals leave nothing behind: a bad row, a row of another record, a check on the totals.
    let bad_row = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "lines": [{ "item": "x", "qty": 0 }] } }),
    )
    .await;
    assert!(matches!(&bad_row, Err(_)), "{bad_row:?}");
    let other = kernel_command(&ctx, "db::create", json!({ "model": "invoice", "data": { "lines": [{ "item": "solo", "qty": 1, "price": "1" }] } })).await?;
    let foreign = other["data"]["lines"][0]["id"].clone();
    let stolen = kernel_command(&ctx, "db::update", json!({ "model": "invoice", "id": id, "data": { "lines": [{ "id": foreign, "item": "solo" }] } })).await;
    assert!(
        matches!(&stolen, Err(HostError::InvalidPayload(m)) if m.contains("not a row of this record")),
        "{stolen:?}"
    );
    let negative = kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "invoice", "id": id, "data": { "discount": "9999" } }),
    )
    .await;
    assert!(
        matches!(&negative, Err(HostError::InvalidPayload(m)) if m.contains("cannot be negative")),
        "{negative:?}"
    );
    let unchanged = kernel_command(
        &ctx,
        "db::get",
        json!({ "model": "invoice", "id": id, "expand": ["lines"] }),
    )
    .await?;
    assert_eq!(
        unchanged["data"]["lines"].as_array().map(Vec::len),
        Some(3),
        "a refused update changed no row"
    );
    assert_eq!(unchanged["data"]["discount"], "10.00");

    // Deleting the record deletes its rows.
    kernel_command(&ctx, "db::delete", json!({ "model": "invoice", "id": id })).await?;
    let rows_left = world
        .raw(&models[1].model_id.clone().unwrap_or_default())
        .await?;
    assert_eq!(rows_left.len(), 1, "only the other invoice's row is left");
    assert!(
        kernel_command(&ctx, "db::get", json!({ "model": "invoice", "id": id })).await?["data"]
            .is_null()
    );
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

    async fn as_user(
        world: &World,
        model: &ModelDef,
        user: &str,
        roles: &[&str],
    ) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
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
                let mut grant = ModelGrant::from_access(
                    &name,
                    &["read".into(), "write".into()],
                    Some(&schema.table),
                );
                grant.schema = Some(schema);
                grant.rules = Some(rules.clone());
                (name, grant)
            })
            .collect();
        Ok(PluginHostContext::new(
            "desk",
            ["db::query".to_string(), "db::mutate".to_string()]
                .into_iter()
                .collect(),
            grants,
            Arc::new(world.session.clone()),
            DbScope::new(NAMESPACE, &world.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext {
                    actor: Actor::User(user.into()),
                    request_id: "r".into(),
                    ip: None,
                    user_agent: None,
                },
                "f",
            ),
        ))
    }

    fn names(reply: &Value) -> Vec<String> {
        reply["data"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn go(ctx: &PluginHostContext, id: &Value, to: &str) -> Result<Value, HostError> {
        kernel_command(
            ctx,
            "db::update",
            json!({ "model": "leave", "id": id, "data": { "state": to } }),
        )
        .await
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn records_move_only_along_transitions_the_caller_may_make() -> TestResult {
        let Some(world) = World::new("workflow").await? else {
            return Ok(());
        };
        let model = leave_model()?;
        let problems = rules()?.problems(&model);
        assert!(problems.is_empty(), "{problems:?}");
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        let root = as_user(&world, &model, "users:root", &["org_admin"]).await?;

        // A new record starts as `draft`, whatever the caller asks for.
        let refused = kernel_command(
            &ann,
            "db::create",
            json!({ "model": "leave", "data": { "employee": "users:ann", "state": "approved" } }),
        )
        .await;
        assert!(
            matches!(&refused, Err(HostError::Denied(m)) if m.contains("starts as draft")),
            "{refused:?}"
        );
        let made = kernel_command(
            &ann,
            "db::create",
            json!({ "model": "leave", "data": { "employee": "users:ann" } }),
        )
        .await?;
        let id = made["data"]["id"].clone();
        assert_eq!(made["data"]["state"], "draft");

        // The owner may submit, and sees exactly that button; nothing skips a step.
        assert_eq!(
            names(
                &kernel_command(
                    &ann,
                    "db::transitions",
                    json!({ "model": "leave", "id": id })
                )
                .await?
            ),
            ["submit"]
        );
        let skipped = go(&ann, &id, "approved").await;
        assert!(matches!(&skipped, Err(HostError::Denied(_))), "{skipped:?}");
        assert_eq!(
            go(&ann, &id, "submitted").await?["data"]["state"],
            "submitted"
        );
        let back = go(&ann, &id, "draft").await;
        assert!(
            matches!(&back, Err(HostError::Denied(m)) if m.contains("from its state now")),
            "{back:?}"
        );
        // Writing the state it already has is not a move.
        assert_eq!(
            go(&ann, &id, "submitted").await?["data"]["state"],
            "submitted"
        );
        assert!(
            names(
                &kernel_command(
                    &ann,
                    "db::transitions",
                    json!({ "model": "leave", "id": id })
                )
                .await?
            )
            .is_empty()
        );

        // An approver sees approve and reject, and approving is a move she may make.
        assert_eq!(
            names(
                &kernel_command(
                    &carol,
                    "db::transitions",
                    json!({ "model": "leave", "id": id })
                )
                .await?
            ),
            ["approve", "reject"]
        );
        assert_eq!(
            go(&carol, &id, "approved").await?["data"]["state"],
            "approved"
        );
        let reopen = go(&carol, &id, "draft").await;
        assert!(reopen.is_err(), "approved is final: {reopen:?}");

        // The workflow's own condition: nobody approves their own request.
        let mine = kernel_command(
            &carol,
            "db::create",
            json!({ "model": "leave", "data": { "employee": "users:carol" } }),
        )
        .await?;
        let mine_id = mine["data"]["id"].clone();
        go(&carol, &mine_id, "submitted").await?;
        assert_eq!(
            names(
                &kernel_command(
                    &carol,
                    "db::transitions",
                    json!({ "model": "leave", "id": mine_id })
                )
                .await?
            ),
            ["reject"]
        );
        let own = go(&carol, &mine_id, "approved").await;
        assert!(matches!(&own, Err(HostError::Denied(_))), "{own:?}");

        // Rejected can be reopened by anyone who can change it; an administrator is not held to it.
        assert_eq!(
            go(&carol, &mine_id, "rejected").await?["data"]["state"],
            "rejected"
        );
        assert_eq!(
            go(&carol, &mine_id, "draft").await?["data"]["state"],
            "draft"
        );
        assert_eq!(go(&root, &id, "draft").await?["data"]["state"], "draft");

        // Every move was written in the record's history by the kernel.
        let lines = world.raw("chatter_messages").await?;
        assert!(
            lines.iter().filter(|line| line["kind"] == "change").count() >= 4,
            "{lines:?}"
        );
        Ok(())
    }
}

// ---- record versions (history) -------------------------------------------------------------

fn ledger(history: Value) -> ModelDef {
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "ledger",
        "history": history,
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "points", "type": "int" },
            { "name": "notes", "type": "text" }
        ]
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    sync_ids(&mut model);
    model
}

async fn versions_of(world: &World) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut response = world
        .session
        .query("SELECT * FROM record_versions ORDER BY model_id, record_key, seq;")
        .await?
        .check()?;
    Ok(response.take(0)?)
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn history_keeps_snapshots_and_deltas_with_each_write() -> TestResult {
    let Some(world) = World::new("history").await? else {
        return Ok(());
    };
    let model = ledger(json!({ "enabled": true, "snapshot_every": 3 }));
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let (title, points, notes) = (
        id_of(&model, "title"),
        id_of(&model, "points"),
        id_of(&model, "notes"),
    );

    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ledger", "data": { "title": "A", "points": 1, "notes": "n" } }),
    )
    .await?;
    let id = created["data"]["id"].clone();
    let rows = versions_of(&world).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["op"], "create");
    assert_eq!(rows[0]["seq"], 1);
    assert_eq!(rows[0]["actor"], "users:u1");
    assert_eq!(rows[0]["snapshot"][&points], 1, "a full snapshot, by field id");
    assert!(rows[0]["delta"].is_null());

    // A write that changes nothing leaves no version.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "title": "A" } }),
    )
    .await?;
    assert_eq!(versions_of(&world).await?.len(), 1);

    // Between snapshots a version holds only what changed; an emptied field is `cleared`.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "points": 2 } }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "title": "B", "notes": null } }),
    )
    .await?;
    let rows = versions_of(&world).await?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["seq"], 2);
    assert_eq!(rows[1]["delta"], json!({ points.as_str(): 2 }));
    assert!(rows[1]["snapshot"].is_null());
    assert_eq!(rows[2]["delta"], json!({ title.as_str(): "B" }));
    assert_eq!(rows[2]["cleared"], json!([notes.as_str()]));

    // The 4th version is a full snapshot again (every 3rd after the first).
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "points": 9 } }),
    )
    .await?;
    let rows = versions_of(&world).await?;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[3]["snapshot"][&points], 9);
    assert_eq!(rows[3]["snapshot"][&title], "B");
    assert_eq!(
        rows[3]["delta"],
        json!({ points.as_str(): 9 }),
        "a snapshot version still says what changed"
    );

    kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": id })).await?;
    let rows = versions_of(&world).await?;
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[4]["op"], "delete");
    assert!(rows[4]["snapshot"].is_null() && rows[4]["delta"].is_null());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn history_starts_a_record_that_predates_it_with_a_baseline() -> TestResult {
    let Some(world) = World::new("history_baseline").await? else {
        return Ok(());
    };
    let off = ledger(json!({ "enabled": false }));
    let mut on = off.clone();
    on.history = Some(serde_json::from_value(json!({ "enabled": true }))?);
    world.apply(std::slice::from_ref(&off)).await?;

    let created = kernel_command(
        &world.ctx(std::slice::from_ref(&off)),
        "db::create",
        json!({ "model": "ledger", "data": { "title": "Old", "points": 1 } }),
    )
    .await?;
    assert!(versions_of(&world).await?.is_empty(), "history was off");
    let id = created["data"]["id"].clone();

    kernel_command(
        &world.ctx(std::slice::from_ref(&on)),
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "points": 2 } }),
    )
    .await?;
    let rows = versions_of(&world).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["op"], "baseline");
    assert_eq!(rows[0]["snapshot"][id_of(&on, "points")], 1, "the state before the write");
    assert_eq!(rows[1]["op"], "update");
    assert_eq!(rows[1]["seq"], 2);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn history_keeps_only_the_named_fields_and_none_for_a_model_without_it() -> TestResult {
    let Some(world) = World::new("history_fields").await? else {
        return Ok(());
    };
    let narrow = ledger(json!({ "enabled": true, "fields": ["points"] }));
    world.apply(std::slice::from_ref(&narrow)).await?;
    let ctx = world.ctx(std::slice::from_ref(&narrow));
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ledger", "data": { "title": "A", "points": 1 } }),
    )
    .await?;
    let id = created["data"]["id"].clone();
    let rows = versions_of(&world).await?;
    let snapshot = rows[0]["snapshot"].as_object().ok_or("no snapshot")?;
    assert_eq!(snapshot.len(), 1, "only `points`: {snapshot:?}");

    // A change to an unkept field is not a version; a change to a kept one is.
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "title": "Z" } }),
    )
    .await?;
    assert_eq!(versions_of(&world).await?.len(), 1);
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ledger", "id": id, "data": { "points": 5 } }),
    )
    .await?;
    assert_eq!(versions_of(&world).await?.len(), 2);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn history_in_a_transaction_versions_each_write_separately() -> TestResult {
    let Some(world) = World::new("history_tx").await? else {
        return Ok(());
    };
    let model = ledger(json!({ "enabled": true }));
    world.apply(std::slice::from_ref(&model)).await?;
    let mut ctx = world.ctx(std::slice::from_ref(&model));
    ctx.granted_capabilities.insert("db::transaction".into());
    let one = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ledger", "data": { "title": "One", "points": 1 } }),
    )
    .await?;
    let two = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ledger", "data": { "title": "Two", "points": 1 } }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::transaction",
        json!({ "ops": [
            { "op": "update", "model": "ledger", "id": one["data"]["id"], "data": { "points": 10 } },
            { "op": "update", "model": "ledger", "id": two["data"]["id"], "data": { "points": 20 } },
            { "op": "create", "model": "ledger", "data": { "title": "Three" } },
        ] }),
    )
    .await?;
    let rows = versions_of(&world).await?;
    // 2 creates + 2 updates + 1 create, each under its own record.
    assert_eq!(rows.len(), 5, "{rows:?}");
    let points = id_of(&model, "points");
    let updates: Vec<&Value> = rows.iter().filter(|r| r["op"] == "update").collect();
    assert_eq!(updates.len(), 2);
    let mut values: Vec<i64> = updates
        .iter()
        .filter_map(|r| r["delta"][&points].as_i64())
        .collect();
    values.sort();
    assert_eq!(values, vec![10, 20]);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_failed_write_leaves_no_version() -> TestResult {
    let Some(world) = World::new("history_rollback").await? else {
        return Ok(());
    };
    let model = ledger(json!({ "enabled": true }));
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    // `title` is required: the create is refused and nothing is written, versions included.
    let refused = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ledger", "data": { "points": 1 } }),
    )
    .await;
    assert!(refused.is_err());
    assert!(versions_of(&world).await?.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_model_without_history_writes_no_versions() -> TestResult {
    let Some(world) = World::new("nohistory").await? else {
        return Ok(());
    };
    let model = ticket();
    world.apply(std::slice::from_ref(&model)).await?;
    let ctx = world.ctx(std::slice::from_ref(&model));
    let created = kernel_command(
        &ctx,
        "db::create",
        json!({ "model": "ticket", "data": { "title": "x" } }),
    )
    .await?;
    kernel_command(
        &ctx,
        "db::update",
        json!({ "model": "ticket", "id": created["data"]["id"], "data": { "points": 1 } }),
    )
    .await?;
    assert!(versions_of(&world).await?.is_empty());
    Ok(())
}

// ---- reading the past ----------------------------------------------------------------------

mod history_reads {
    use std::sync::Arc;

    use aether_core::data_model::RuleSet;

    use super::*;

    fn with_history_cap(mut ctx: PluginHostContext) -> PluginHostContext {
        ctx.granted_capabilities.insert("history::read".into());
        ctx
    }

    /// Create a ledger record and change it twice, then delete it. Returns its id.
    async fn lifecycle(ctx: &PluginHostContext) -> Result<Value, Box<dyn std::error::Error>> {
        let created = kernel_command(
            ctx,
            "db::create",
            json!({ "model": "ledger", "data": { "title": "A", "points": 1, "notes": "n" } }),
        )
        .await?;
        let id = created["data"]["id"].clone();
        kernel_command(
            ctx,
            "db::update",
            json!({ "model": "ledger", "id": id, "data": { "points": 2 } }),
        )
        .await?;
        kernel_command(
            ctx,
            "db::update",
            json!({ "model": "ledger", "id": id, "data": { "title": "B", "notes": null } }),
        )
        .await?;
        Ok(id)
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn a_record_can_be_read_as_it_was_by_version_or_by_time() -> TestResult {
        let Some(world) = World::new("asof").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true, "snapshot_every": 2 }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&model)));
        let id = lifecycle(&ctx).await?;
        let get = |as_of: Value| {
            let (ctx, id) = (&ctx, id.clone());
            async move {
                kernel_command(
                    ctx,
                    "db::get",
                    json!({ "model": "ledger", "id": id, "as_of": as_of }),
                )
                .await
            }
        };

        let v1 = get(json!({ "seq": 1 })).await?;
        assert_eq!(v1["data"]["points"], 1);
        assert_eq!(v1["data"]["title"], "A");
        assert_eq!(v1["data"]["notes"], "n");
        assert!(v1["data"].get("__seq").is_none(), "no bookkeeping leaks: {v1}");
        let v2 = get(json!({ "seq": 2 })).await?;
        assert_eq!((v2["data"]["points"].clone(), v2["data"]["title"].clone()), (json!(2), json!("A")));
        // Across a snapshot (every 2nd version) and a cleared field.
        let v3 = get(json!({ "seq": 3 })).await?;
        assert_eq!(v3["data"]["title"], "B");
        assert!(v3["data"].get("notes").is_none(), "cleared: {v3}");

        // By time: at version 2's own time it is version 2; at version 1's, version 1.
        let rows = versions_of(&world).await?;
        let at = |n: usize| rows[n]["at"].clone();
        assert_eq!(get(at(1)).await?["data"]["points"], 2);
        assert_eq!(get(at(0)).await?["data"]["points"], 1);
        assert_eq!(get(at(1)).await?["as_of_seq"], 2);
        // Long before it existed: nothing (it was created with history on, so it has no baseline).
        assert!(get(json!("2000-01-01T00:00:00Z")).await?["data"].is_null());
        // A version that does not exist.
        assert!(get(json!({ "seq": 99 })).await?["data"].is_null());
        // Plain reads are unchanged by all this.
        let now = kernel_command(&ctx, "db::get", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(now["data"]["title"], "B");

        // After the delete the past is still readable, the present is gone.
        kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(get(json!({ "seq": 2 })).await?["data"]["points"], 2);
        assert!(get(json!({ "seq": 4 })).await?["data"].is_null(), "the delete itself");

        // Bad input is refused, not guessed.
        for bad in [json!("yesterday"), json!({ "seq": 0 }), json!(5), json!({ "x": 1 })] {
            let refused = get(bad.clone()).await;
            assert!(matches!(refused, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{bad}: {refused:?}");
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn the_baseline_answers_for_times_before_history_began() -> TestResult {
        let Some(world) = World::new("asof_base").await? else {
            return Ok(());
        };
        let off = ledger(json!({ "enabled": false }));
        let mut on = off.clone();
        on.history = Some(serde_json::from_value(json!({ "enabled": true }))?);
        world.apply(std::slice::from_ref(&off)).await?;
        let created = kernel_command(
            &world.ctx(std::slice::from_ref(&off)),
            "db::create",
            json!({ "model": "ledger", "data": { "title": "Old", "points": 1 } }),
        )
        .await?;
        let id = created["data"]["id"].clone();
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&on)));
        kernel_command(
            &ctx,
            "db::update",
            json!({ "model": "ledger", "id": id, "data": { "points": 2 } }),
        )
        .await?;
        let before = kernel_command(
            &ctx,
            "db::get",
            json!({ "model": "ledger", "id": id, "as_of": "2000-01-01T00:00:00Z" }),
        )
        .await?;
        assert_eq!(before["data"]["points"], 1, "{before}");
        assert_eq!(before["from_baseline"], true);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn the_list_and_the_diff_say_who_what_and_when() -> TestResult {
        let Some(world) = World::new("hlist").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true, "snapshot_every": 2 }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&model)));
        let id = lifecycle(&ctx).await?;

        let list = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id })).await?;
        let versions = list["data"].as_array().ok_or("no list")?;
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0]["seq"], 3, "newest first");
        assert_eq!(versions[0]["actor"], "users:u1");
        assert_eq!(versions[0]["fields"], json!(["notes", "title"]), "names, not ids");
        assert_eq!(versions[1]["fields"], json!(["points"]), "a snapshot version still says what changed");
        assert_eq!(versions[2]["op"], "create");
        assert_eq!(versions[2]["fields"], json!(["notes", "points", "title"]));

        // Paging.
        let page = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id, "limit": 2 })).await?;
        assert_eq!(page["data"].as_array().map(Vec::len), Some(2));
        assert_eq!(page["next"], 2);
        let rest = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id, "limit": 2, "before_seq": 2 })).await?;
        assert_eq!(rest["data"][0]["seq"], 1);
        assert!(rest["next"].is_null());

        let diff = kernel_command(&ctx, "history::diff", json!({ "model": "ledger", "id": id, "from": 1 })).await?;
        assert_eq!(diff["data"]["to"], 3, "the newest by default");
        let changes = diff["data"]["changes"].as_array().ok_or("no changes")?;
        let field = |name: &str| changes.iter().find(|c| c["field"] == name).cloned();
        assert_eq!(field("points").map(|c| (c["before"].clone(), c["after"].clone())), Some((json!(1), json!(2))));
        assert_eq!(field("title").map(|c| c["after"].clone()), Some(json!("B")));
        assert_eq!(field("notes").map(|c| c["after"].clone()), Some(Value::Null), "cleared");

        // Still there after the record is gone; a version that does not exist is nothing.
        kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": id })).await?;
        let after = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(after["data"][0]["op"], "delete");
        let missing = kernel_command(&ctx, "history::diff", json!({ "model": "ledger", "id": id, "from": 1, "to": 99 })).await?;
        assert!(missing["data"].is_null());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn history_needs_its_capability_and_a_model_that_keeps_one() -> TestResult {
        let Some(world) = World::new("hcap").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = world.ctx(std::slice::from_ref(&model)); // no history::read
        let refused = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": "x" })).await;
        assert!(matches!(refused, Err(HostError::Capability(_))), "{refused:?}");

        let plain = ticket();
        world.apply(std::slice::from_ref(&plain)).await?;
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&plain)));
        let none = kernel_command(&ctx, "history::list", json!({ "model": "ticket", "id": "x" })).await;
        assert!(matches!(none, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{none:?}");
        let none = kernel_command(&ctx, "db::get", json!({ "model": "ticket", "id": "x", "as_of": { "seq": 1 } })).await;
        assert!(matches!(none, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{none:?}");
        Ok(())
    }

    fn secret_ledger() -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "ledger",
            "history": { "enabled": true },
            "fields": [
                { "name": "owner", "type": "string", "required": true },
                { "name": "title", "type": "string" },
                { "name": "notes", "type": "text" }
            ]
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        sync_ids(&mut model);
        model
    }

    async fn as_user(
        world: &World,
        model: &ModelDef,
        user: &str,
        roles: &[&str],
    ) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
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
        let rules = RuleSet::parse(
            &json!({
                "model": "ledger",
                "access": [
                    { "name": "own", "operations": ["read", "create", "write", "delete"], "when": { "owner": "$user" } },
                    { "name": "approvers", "roles": ["approver"], "operations": ["read"] }
                ],
                "fields": [ { "fields": ["notes"], "read_roles": ["approver"] } ]
            })
            .to_string(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let grants: HashMap<String, ModelGrant> = schemas_of(std::slice::from_ref(model))
            .into_iter()
            .map(|(name, schema)| {
                let mut grant = ModelGrant::from_access(
                    &name,
                    &["read".into(), "write".into()],
                    Some(&schema.table),
                );
                grant.schema = Some(schema);
                grant.rules = Some(Arc::new(rules.clone()));
                (name, grant)
            })
            .collect();
        Ok(PluginHostContext::new(
            "desk",
            [
                "db::query".to_string(),
                "db::mutate".to_string(),
                "history::read".to_string(),
            ]
            .into_iter()
            .collect(),
            grants,
            Arc::new(world.session.clone()),
            DbScope::new(NAMESPACE, &world.org),
            NotificationHub::default(),
            CallInfo::new(
                AuditContext {
                    actor: Actor::User(user.into()),
                    request_id: "r".into(),
                    ip: None,
                    user_agent: None,
                },
                "f",
            ),
        ))
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn the_past_is_shown_under_todays_rules() -> TestResult {
        let Some(world) = World::new("hrules").await? else {
            return Ok(());
        };
        let model = secret_ledger();
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &[]).await?;
        let bob = as_user(&world, &model, "users:bob", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;

        let created = kernel_command(
            &ann,
            "db::create",
            json!({ "model": "ledger", "data": { "owner": "users:ann", "title": "T", "notes": "secret" } }),
        )
        .await?;
        let id = created["data"]["id"].clone();
        kernel_command(
            &ann,
            "db::update",
            json!({ "model": "ledger", "id": id, "data": { "title": "T2", "notes": "secret2" } }),
        )
        .await?;

        let at1 = json!({ "model": "ledger", "id": id, "as_of": { "seq": 1 } });
        // The owner sees the past, but not the field her roles hide today.
        let own = kernel_command(&ann, "db::get", at1.clone()).await?;
        assert_eq!(own["data"]["title"], "T");
        assert!(own["data"].get("notes").is_none(), "hidden field stays hidden: {own}");
        // A person who may not read the record today gets nothing from its past either.
        assert!(kernel_command(&bob, "db::get", at1.clone()).await?["data"].is_null());
        let hidden = kernel_command(&bob, "history::list", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(hidden["data"], json!([]));
        let none = kernel_command(&bob, "history::diff", json!({ "model": "ledger", "id": id, "from": 1, "to": 2 })).await?;
        assert!(none["data"].is_null());
        // An approver may read everything.
        let all = kernel_command(&carol, "db::get", at1).await?;
        assert_eq!(all["data"]["notes"], "secret");

        // The list and the diff leave out the field she cannot read.
        let list = kernel_command(&ann, "history::list", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(list["data"][0]["fields"], json!(["title"]));
        let diff = kernel_command(&ann, "history::diff", json!({ "model": "ledger", "id": id, "from": 1, "to": 2 })).await?;
        let fields: Vec<&str> = diff["data"]["changes"]
            .as_array()
            .ok_or("no changes")?
            .iter()
            .filter_map(|c| c["field"].as_str())
            .collect();
        assert_eq!(fields, vec!["title"]);
        let carols = kernel_command(&carol, "history::diff", json!({ "model": "ledger", "id": id, "from": 1, "to": 2 })).await?;
        assert_eq!(carols["data"]["changes"].as_array().map(Vec::len), Some(2));
        Ok(())
    }

    fn restore_cap(mut ctx: PluginHostContext) -> PluginHostContext {
        ctx.granted_capabilities.insert("history::restore".into());
        ctx
    }

    async fn current(ctx: &PluginHostContext, id: &Value) -> Result<Value, HostError> {
        Ok(kernel_command(ctx, "db::get", json!({ "model": "ledger", "id": id })).await?["data"].clone())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn restore_puts_a_record_back_as_one_new_version() -> TestResult {
        let Some(world) = World::new("restore").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = restore_cap(with_history_cap(world.ctx(std::slice::from_ref(&model))));
        let id = lifecycle(&ctx).await?; // v1 A/1/n, v2 points 2, v3 title B and notes cleared

        let done = kernel_command(&ctx, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await?;
        assert_eq!(done["restored"], true);
        assert_eq!(done["from_seq"], 1);
        let now = current(&ctx, &id).await?;
        assert_eq!((now["title"].clone(), now["points"].clone(), now["notes"].clone()), (json!("A"), json!(1), json!("n")));

        // One new version, labelled, which the history lists like any other.
        let rows = versions_of(&world).await?;
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[3]["op"], "restore");
        assert_eq!(rows[3]["restored_from"], 1);
        assert_eq!(rows[3]["seq"], 4);
        let list = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id, "limit": 1 })).await?;
        assert_eq!(list["data"][0]["op"], "restore");

        // Nothing differs now: nothing is written.
        let again = kernel_command(&ctx, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await?;
        assert_eq!(again["restored"], false);
        assert_eq!(versions_of(&world).await?.len(), 4);

        // A version that is not there is refused.
        let missing = kernel_command(&ctx, "history::restore", json!({ "model": "ledger", "id": id, "seq": 99 })).await;
        assert!(matches!(missing, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{missing:?}");
        // A deleted record is not brought back.
        kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": id })).await?;
        let gone = kernel_command(&ctx, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await;
        assert!(matches!(gone, Err(HostError::Denied(_))), "{gone:?}");
        // The capability is needed.
        let plain = with_history_cap(world.ctx(std::slice::from_ref(&model)));
        let refused = kernel_command(&plain, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await;
        assert!(matches!(refused, Err(HostError::Capability(_))), "{refused:?}");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn undo_takes_back_your_own_last_change_and_only_that() -> TestResult {
        let Some(world) = World::new("undo").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = restore_cap(with_history_cap(world.ctx(std::slice::from_ref(&model))));
        let id = lifecycle(&ctx).await?;

        let undone = kernel_command(&ctx, "history::undo", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(undone["restored"], true);
        let now = current(&ctx, &id).await?;
        assert_eq!((now["title"].clone(), now["notes"].clone()), (json!("A"), json!("n")), "back to version 2");
        assert_eq!(now["points"], 2);
        // Undoing the undo puts the change back.
        kernel_command(&ctx, "history::undo", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(current(&ctx, &id).await?["title"], "B");

        // Someone else's change is not yours to undo.
        let mut other = ctx.clone();
        other.audit.actor = Actor::User("users:u2".into());
        kernel_command(&other, "db::update", json!({ "model": "ledger", "id": id, "data": { "points": 7 } })).await?;
        let refused = kernel_command(&ctx, "history::undo", json!({ "model": "ledger", "id": id })).await;
        assert!(matches!(refused, Err(HostError::Denied(_))), "{refused:?}");
        assert_eq!(current(&ctx, &id).await?["points"], 7);

        // Too long ago.
        world.session.query("UPDATE record_versions SET at = time::now() - 1h;").await?.check()?;
        let late = kernel_command(&other, "history::undo", json!({ "model": "ledger", "id": id })).await;
        assert!(matches!(late, Err(HostError::Denied(_))), "{late:?}");
        // A creation cannot be undone.
        let fresh = kernel_command(&ctx, "db::create", json!({ "model": "ledger", "data": { "title": "N" } })).await?;
        let fresh = fresh["data"]["id"].clone();
        let nothing = kernel_command(&ctx, "history::undo", json!({ "model": "ledger", "id": fresh })).await;
        assert!(matches!(nothing, Err(HostError::Denied(_))), "{nothing:?}");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn restore_leaves_alone_what_the_caller_cannot_read() -> TestResult {
        let Some(world) = World::new("restore_rules").await? else {
            return Ok(());
        };
        let model = secret_ledger();
        world.apply(std::slice::from_ref(&model)).await?;
        let mut ann = as_user(&world, &model, "users:ann", &[]).await?;
        ann.granted_capabilities.insert("history::restore".into());
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        let created = kernel_command(
            &ann,
            "db::create",
            json!({ "model": "ledger", "data": { "owner": "users:ann", "title": "T", "notes": "secret" } }),
        )
        .await?;
        let id = created["data"]["id"].clone();
        kernel_command(&ann, "db::update", json!({ "model": "ledger", "id": id, "data": { "title": "T2", "notes": "secret2" } })).await?;

        let done = kernel_command(&ann, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await?;
        assert_eq!(done["restored"], true);
        assert_eq!(done["data"]["title"], "T");
        // The notes she cannot read were not touched.
        let seen = kernel_command(&carol, "db::get", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(seen["data"]["title"], "T");
        assert_eq!(seen["data"]["notes"], "secret2");

        // Someone with no write access cannot restore at all.
        let mut carol = carol;
        carol.granted_capabilities.insert("history::restore".into());
        let refused = kernel_command(&carol, "history::restore", json!({ "model": "ledger", "id": id, "seq": 1 })).await;
        assert!(refused.is_err(), "an approver may read but not write: {refused:?}");
        Ok(())
    }

    // ---- find as_of, redaction, retention, the undo action ----------------------------------

    /// The time of the most recent version of any record.
    async fn newest_at(world: &World) -> Result<Value, Box<dyn std::error::Error>> {
        let rows = versions_of(world).await?;
        rows.iter()
            .filter_map(|r| r["at"].as_str())
            .max()
            .map(|at| json!(at))
            .ok_or_else(|| "no versions".into())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn find_as_of_searches_the_old_states() -> TestResult {
        let Some(world) = World::new("findasof").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = restore_cap(with_history_cap(world.ctx(std::slice::from_ref(&model))));
        let make = |title: &str, points: i64| {
            let (ctx, title) = (&ctx, title.to_string());
            async move {
                kernel_command(ctx, "db::create", json!({ "model": "ledger", "data": { "title": title, "points": points } }))
                    .await
                    .map(|r| r["data"]["id"].clone())
            }
        };
        let a = make("A", 1).await?;
        let b = make("B", 5).await?;
        let t1 = newest_at(&world).await?;
        kernel_command(&ctx, "db::update", json!({ "model": "ledger", "id": a, "data": { "points": 9 } })).await?;
        kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": b })).await?;
        let t2 = newest_at(&world).await?;
        let find = |at: Value, filter: Value| {
            let ctx = &ctx;
            async move {
                kernel_command(ctx, "db::find", json!({ "model": "ledger", "as_of": at, "filter": filter })).await
            }
        };
        let titles = |reply: &Value| -> Vec<String> {
            let mut t: Vec<String> = reply["data"].as_array().map(|rows| rows.iter().filter_map(|r| r["title"].as_str().map(str::to_string)).collect()).unwrap_or_default();
            t.sort();
            t
        };

        assert_eq!(titles(&find(t1.clone(), json!({})).await?), ["A", "B"]);
        assert_eq!(titles(&find(t1.clone(), json!({ "points": { "gte": 5 } })).await?), ["B"], "A was 1 then");
        assert_eq!(titles(&find(t2.clone(), json!({})).await?), ["A"], "B was deleted by then");
        assert_eq!(titles(&find(t2.clone(), json!({ "points": { "gte": 5 } })).await?), ["A"], "A is 9 now");
        let limited = kernel_command(&ctx, "db::find", json!({ "model": "ledger", "as_of": t1, "limit": 1 })).await?;
        assert_eq!(limited["data"].as_array().map(Vec::len), Some(1));
        // Before anything existed: nothing.
        assert!(find(json!("2000-01-01T00:00:00Z"), json!({})).await?["data"].as_array().is_some_and(|r| r.is_empty()));

        // What it does not do is refused, not guessed.
        for bad in [
            json!({ "model": "ledger", "as_of": { "seq": 1 } }),
            json!({ "model": "ledger", "as_of": "2030-01-01T00:00:00Z", "order": "title" }),
            json!({ "model": "ledger", "as_of": "2030-01-01T00:00:00Z", "offset": 1 }),
            json!({ "model": "ledger", "as_of": "2030-01-01T00:00:00Z", "expand": ["x"] }),
        ] {
            let refused = kernel_command(&ctx, "db::find", bad.clone()).await;
            assert!(matches!(refused, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{bad}: {refused:?}");
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn find_as_of_shows_only_what_todays_rules_allow() -> TestResult {
        let Some(world) = World::new("findrules").await? else {
            return Ok(());
        };
        let model = secret_ledger();
        world.apply(std::slice::from_ref(&model)).await?;
        let ann = as_user(&world, &model, "users:ann", &[]).await?;
        let bob = as_user(&world, &model, "users:bob", &[]).await?;
        let carol = as_user(&world, &model, "users:carol", &["desk.approver"]).await?;
        kernel_command(&ann, "db::create", json!({ "model": "ledger", "data": { "owner": "users:ann", "title": "T", "notes": "secret" } })).await?;
        let at = versions_of(&world).await?[0]["at"].clone();
        let ask = |ctx: &PluginHostContext| {
            let at = at.clone();
            let ctx = ctx.clone();
            async move { kernel_command(&ctx, "db::find", json!({ "model": "ledger", "as_of": at })).await }
        };
        let own = ask(&ann).await?;
        assert_eq!(own["data"].as_array().map(Vec::len), Some(1));
        assert!(own["data"][0].get("notes").is_none(), "hidden field stays hidden");
        assert_eq!(ask(&bob).await?["data"].as_array().map(Vec::len), Some(0), "not bob's record");
        assert_eq!(ask(&carol).await?["data"][0]["notes"], "secret", "an approver reads all");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn redact_blots_out_a_field_everywhere_in_the_history() -> TestResult {
        let Some(world) = World::new("redact").await? else {
            return Ok(());
        };
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "ledger",
            "history": { "enabled": true },
            "chatter": { "enabled": true },
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "secret", "type": "string", "track": true }
            ]
        }))?;
        sync_ids(&mut model);
        world.apply(std::slice::from_ref(&model)).await?;
        let mut ctx = with_history_cap(world.ctx(std::slice::from_ref(&model)));
        ctx.granted_capabilities.insert("history::redact".into());
        let created = kernel_command(&ctx, "db::create", json!({ "model": "ledger", "data": { "title": "T", "secret": "one" } })).await?;
        let id = created["data"]["id"].clone();
        kernel_command(&ctx, "db::update", json!({ "model": "ledger", "id": id, "data": { "secret": "two" } })).await?;
        kernel_command(&ctx, "db::update", json!({ "model": "ledger", "id": id, "data": { "title": "T2" } })).await?;
        let secret = id_of(&model, "secret");

        // Not an administrator: refused, and nothing changes.
        let refused = kernel_command(&ctx, "history::redact", json!({ "model": "ledger", "id": id, "fields": ["secret"] })).await;
        assert!(matches!(refused, Err(HostError::Denied(_))), "{refused:?}");
        assert!(serde_json::to_string(&versions_of(&world).await?)?.contains("one"));

        ctx.bypass_rules = true; // stands for org_admin
        let done = kernel_command(&ctx, "history::redact", json!({ "model": "ledger", "id": id, "fields": ["secret"], "reason": "erasure request" })).await?;
        assert_eq!(done["data"]["redacted"], json!(["secret"]));
        let dump = serde_json::to_string(&versions_of(&world).await?)?;
        assert!(!dump.contains("\"one\"") && !dump.contains("\"two\""), "gone from the versions: {dump}");
        let chatter = serde_json::to_string(&world.raw("chatter_messages").await?)?;
        assert!(!chatter.contains("\"one\"") && !chatter.contains("\"two\""), "gone from the chatter: {chatter}");
        assert!(chatter.contains("[redacted]"));
        // The history is still there, with the field blotted out; the other fields are untouched.
        let old = kernel_command(&ctx, "db::get", json!({ "model": "ledger", "id": id, "as_of": { "seq": 2 } })).await?;
        assert_eq!(old["data"]["secret"], "[redacted]");
        assert_eq!(old["data"]["title"], "T");
        let list = kernel_command(&ctx, "history::list", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(list["data"].as_array().map(Vec::len), Some(3));
        // The log says who did it, to what, and why.
        let log = world.raw("history_redactions").await?;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["fields"], json!([secret]));
        assert_eq!(log[0]["reason"], "erasure request");
        assert_eq!(log[0]["actor"], "users:u1");
        // Bad input.
        let none = kernel_command(&ctx, "history::redact", json!({ "model": "ledger", "id": id, "fields": [] })).await;
        assert!(matches!(none, Err(HostError::InvalidPayload(_) | HostError::Refused(_))), "{none:?}");
        let unknown = kernel_command(&ctx, "history::redact", json!({ "model": "ledger", "id": id, "fields": ["nope"] })).await;
        assert!(unknown.is_err());
        Ok(())
    }

    /// Make every version of every record this old.
    async fn age(world: &World, days: u32) -> TestResult {
        world
            .session
            .query(format!("UPDATE record_versions SET at = time::now() - {days}d;"))
            .await?
            .check()?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn old_versions_are_folded_into_a_snapshot_and_the_record_stays_readable() -> TestResult {
        let Some(world) = World::new("retain").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true, "keep_days": 7 }));
        world.apply(std::slice::from_ref(&model)).await?;
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&model)));
        let id = lifecycle(&ctx).await?; // v1, v2, v3
        // v1 and v2 are 10 days old; v3 is recent.
        world.session.query("UPDATE record_versions SET at = time::now() - 10d WHERE seq <= 2;").await?.check()?;

        let removed = aether_core_api::history_retention::purge(&world.session, 0).await?;
        assert_eq!(removed, 1, "v1 went");
        let rows = versions_of(&world).await?;
        assert_eq!(rows.iter().map(|r| r["seq"].as_u64().unwrap_or(0)).collect::<Vec<_>>(), vec![2, 3]);
        assert!(rows[0]["snapshot"].is_object(), "v2 is now a full snapshot");
        // The record can still be read as it was from v2 on.
        let v2 = kernel_command(&ctx, "db::get", json!({ "model": "ledger", "id": id, "as_of": { "seq": 2 } })).await?;
        assert_eq!((v2["data"]["points"].clone(), v2["data"]["title"].clone(), v2["data"]["notes"].clone()), (json!(2), json!("A"), json!("n")));
        let v3 = kernel_command(&ctx, "db::get", json!({ "model": "ledger", "id": id, "as_of": { "seq": 3 } })).await?;
        assert_eq!(v3["data"]["title"], "B");
        assert!(v3["data"].get("notes").is_none());
        // Version 1 is gone.
        let v1 = kernel_command(&ctx, "db::get", json!({ "model": "ledger", "id": id, "as_of": { "seq": 1 } })).await?;
        assert!(v1["data"].is_null());
        // Nothing more to do.
        assert_eq!(aether_core_api::history_retention::purge(&world.session, 0).await?, 0);

        // A deleted record loses its whole history once it is old enough.
        kernel_command(&ctx, "db::delete", json!({ "model": "ledger", "id": id })).await?;
        age(&world, 10).await?;
        let removed = aether_core_api::history_retention::purge(&world.session, 0).await?;
        assert_eq!(removed, 3, "v2, v3 and the delete");
        assert!(versions_of(&world).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn the_organization_setting_applies_only_to_models_without_their_own() -> TestResult {
        let Some(world) = World::new("retain_default").await? else {
            return Ok(());
        };
        let forever = ledger(json!({ "enabled": true, "keep_days": 0 }));
        world.apply(std::slice::from_ref(&forever)).await?;
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&forever)));
        lifecycle(&ctx).await?;
        age(&world, 400).await?;
        // `keep_days: 0` is forever, whatever the organization says.
        assert_eq!(aether_core_api::history_retention::purge(&world.session, 30).await?, 0);
        assert_eq!(versions_of(&world).await?.len(), 3);

        // A model with no keep time of its own follows the organization.
        let follows = ledger(json!({ "enabled": true }));
        let ctx = with_history_cap(world.ctx(std::slice::from_ref(&follows)));
        let plain = versions_of(&world).await?.len();
        lifecycle(&ctx).await?; // a second record, versions with no keep_days
        world.session.query("UPDATE record_versions SET at = time::now() - 400d WHERE keep_days = NONE;").await?.check()?;
        assert_eq!(aether_core_api::history_retention::purge(&world.session, 0).await?, 0, "0 keeps forever");
        assert_eq!(aether_core_api::history_retention::purge(&world.session, 30).await?, 2, "folded to one");
        assert_eq!(versions_of(&world).await?.len(), plain + 1);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn the_undo_action_needs_no_history_capability_but_keeps_the_rules() -> TestResult {
        let Some(world) = World::new("undoaction").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        // The plugin declares neither history::read nor history::restore.
        let ctx = world.ctx(std::slice::from_ref(&model));
        let id = lifecycle(&ctx).await?;
        let answer = aether_core::plugin_manager::record_actions::run(
            &ctx,
            "kernel.undo_records",
            &json!({ "model": "ledger", "ids": [id] }),
        )
        .await
        .ok_or("not a record action")??;
        assert_eq!(answer["done"].as_array().map(Vec::len), Some(1), "{answer}");
        assert_eq!(current(&ctx, &id).await?["notes"], "n", "back to version 2");
        // The capability was not left on the plugin's own context.
        let refused = kernel_command(&ctx, "history::undo", json!({ "model": "ledger", "id": id })).await;
        assert!(matches!(refused, Err(HostError::Capability(_))), "{refused:?}");
        // A model without a history reports it per record.
        let plain = ticket();
        world.apply(std::slice::from_ref(&plain)).await?;
        let pctx = world.ctx(std::slice::from_ref(&plain));
        let made = kernel_command(&pctx, "db::create", json!({ "model": "ticket", "data": { "title": "x" } })).await?;
        let answer = aether_core::plugin_manager::record_actions::run(
            &pctx,
            "kernel.undo_records",
            &json!({ "model": "ticket", "ids": [made["data"]["id"]] }),
        )
        .await
        .ok_or("not a record action")??;
        assert_eq!(answer["failed"].as_array().map(Vec::len), Some(1), "{answer}");
        Ok(())
    }
}

// ---- dry run: a write checked by the database and taken back --------------------------------

mod dry_run {
    use super::*;

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn a_dry_run_write_is_checked_for_real_and_leaves_nothing() -> TestResult {
        let Some(world) = World::new("dryrun").await? else {
            return Ok(());
        };
        let model = ledger(json!({ "enabled": true }));
        world.apply(std::slice::from_ref(&model)).await?;
        let real = world.ctx(std::slice::from_ref(&model));
        let dry = real.clone().with_dry_run();
        let count = |table: &'static str| {
            let world = &world;
            async move { world.raw(table).await.map(|rows| rows.len()) }
        };

        // Reads in a dry run are real reads (and must not poison what is cached for everyone else).
        kernel_command(&real, "db::create", json!({ "model": "ledger", "data": { "title": "Seen" } })).await?;
        let seen = kernel_command(&dry, "db::find", json!({ "model": "ledger", "filter": { "title": "Seen" } })).await?;
        assert_eq!(seen["data"].as_array().map(Vec::len), Some(1), "a dry run sees what is there");
        let seen_by_all = kernel_command(&real, "db::find", json!({ "model": "ledger", "filter": { "title": "Seen" } })).await?;
        assert_eq!(seen_by_all["data"].as_array().map(Vec::len), Some(1));
        let versions_so_far = world.raw("record_versions").await?.len();

        // A create that would work: accepted, and nothing is there afterwards (no record, audit row or version).
        let audits_before = count("data_access").await?;
        let accepted = kernel_command(&dry, "db::create", json!({ "model": "ledger", "data": { "title": "A", "points": 1 } })).await;
        assert!(accepted.is_ok(), "{accepted:?}");
        assert_eq!(count("record_versions").await?, versions_so_far);
        assert_eq!(count("data_access").await?, audits_before, "not even the audit row");
        let found = kernel_command(&real, "db::find", json!({ "model": "ledger" })).await?;
        assert_eq!(found["data"].as_array().map(Vec::len), Some(1), "only the one that was really made");

        // A create the model refuses is refused for the same reason as for real.
        let refused = kernel_command(&dry, "db::create", json!({ "model": "ledger", "data": { "points": 1 } })).await;
        assert!(refused.is_err(), "a required field is missing: {refused:?}");
        let refused_real = kernel_command(&real, "db::create", json!({ "model": "ledger", "data": { "points": 1 } })).await;
        assert_eq!(
            refused.err().map(|e| e.to_string()),
            refused_real.err().map(|e| e.to_string()),
            "the same words"
        );

        // An update and a delete are checked against the real record, and change nothing.
        let made = kernel_command(&real, "db::create", json!({ "model": "ledger", "data": { "title": "B", "points": 2 } })).await?;
        let id = made["data"]["id"].clone();
        let versions = count("record_versions").await?;
        let changed = kernel_command(&dry, "db::update", json!({ "model": "ledger", "id": id, "data": { "points": 99 } })).await;
        assert!(changed.is_ok(), "{changed:?}");
        let gone = kernel_command(&dry, "db::delete", json!({ "model": "ledger", "id": id })).await;
        assert!(gone.is_ok(), "{gone:?}");
        let now = kernel_command(&real, "db::get", json!({ "model": "ledger", "id": id })).await?;
        assert_eq!(now["data"]["points"], 2, "untouched");
        assert_eq!(count("record_versions").await?, versions, "no new version");

        // Several writes together are taken back together, and one that fails is reported.
        let mut real_with_tx = real.clone();
        real_with_tx.granted_capabilities.insert("db::transaction".into());
        let dry_tx = real_with_tx.clone().with_dry_run();
        let ok = kernel_command(&dry_tx, "db::transaction", json!({ "ops": [
            { "op": "create", "model": "ledger", "data": { "title": "C" } },
            { "op": "update", "model": "ledger", "id": id, "data": { "points": 5 } },
        ] })).await;
        assert!(ok.is_ok(), "{ok:?}");
        let bad = kernel_command(&dry_tx, "db::transaction", json!({ "ops": [
            { "op": "create", "model": "ledger", "data": { "title": "D" } },
            { "op": "create", "model": "ledger", "data": { "points": 1 } },
        ] })).await;
        assert!(bad.is_err(), "{bad:?}");
        let found = kernel_command(&real, "db::find", json!({ "model": "ledger" })).await?;
        assert_eq!(found["data"].as_array().map(Vec::len), Some(2), "Seen and B, and nothing of C or D");
        Ok(())
    }
}

// ---- importing rows ------------------------------------------------------------------------

mod import_rows {
    use aether_core::import::mapping::{Mapping, guess};
    use aether_core::import::rows::{Action, LinkCache, Outcome, apply_batch, resolve};
    use aether_core::import::table::{Limits, Table, parse};

    use super::*;

    fn models() -> Vec<ModelDef> {
        let mut contact: ModelDef = serde_json::from_value(json!({
            "name": "contact",
            "fields": [
                { "name": "name", "type": "string", "required": true },
                { "name": "email", "type": "string", "index": "unique" },
                { "name": "amount", "type": "decimal", "scale": 2 },
                { "name": "status", "type": "select", "default": "new", "options": [{ "value": "new" }, { "value": "won" }] },
                { "name": "joined", "type": "date" },
                { "name": "active", "type": "bool" },
                { "name": "company", "type": "link", "target": "company" }
            ]
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        let mut company: ModelDef = serde_json::from_value(json!({
            "name": "company",
            "fields": [ { "name": "title", "type": "string", "required": true } ]
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        sync_ids(&mut contact);
        sync_ids(&mut company);
        vec![contact, company]
    }

    fn table(csv: &str) -> Table {
        parse(csv.as_bytes(), None, Limits::default()).unwrap_or_else(|e| panic!("{e}"))
    }

    fn mapping_for(table: &Table, model: &ModelDef) -> Mapping {
        guess(&table.headers, model, &std::collections::BTreeMap::new()).mapping
    }

    async fn run(ctx: &PluginHostContext, model: &ModelDef, table: &Table, mapping: &Mapping, stop: bool) -> Vec<Outcome> {
        let resolved = resolve(model, &table.headers, mapping).unwrap_or_else(|e| panic!("{e}"));
        apply_batch(ctx, &resolved, &table.rows, stop, &mut LinkCache::default()).await
    }

    fn actions(outcomes: &[Outcome]) -> Vec<&'static str> {
        outcomes.iter().map(|o| o.action.name()).collect()
    }

    async fn all(ctx: &PluginHostContext, model: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        Ok(kernel_command(ctx, "db::find", json!({ "model": model })).await?["data"].as_array().cloned().unwrap_or_default())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn rows_become_records_and_a_bad_row_is_named_and_does_not_stop_the_rest() -> TestResult {
        let Some(world) = World::new("imp_create").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let ctx = world.ctx(&models);
        let contact = &models[0];
        let t = table("Name,E-mail,Amount,Status,Joined,Active\nAnn,ann@x.com,\"1,234.5\",won,2026-03-01,yes\nBob,bob@x.com,10,,,\n,nobody@x.com,1,new,,\nCy,cy@x.com,5.123,new,,\nDi,di@x.com,7,pending,,\nEd,ed@x.com,3,new,31/02/2026,no\n");
        let outcomes = run(&ctx, contact, &t, &mapping_for(&t, contact), false).await;
        assert_eq!(actions(&outcomes), ["create", "create", "error", "error", "error", "error"], "{outcomes:#?}");
        // The messages say which column and what was expected.
        let msg = |i: usize| outcomes[i].message.clone().unwrap_or_default();
        assert!(msg(3).contains("Amount") && msg(3).contains("more than 2"), "{}", msg(3));
        assert!(msg(4).contains("Status") && msg(4).contains("new, won"), "{}", msg(4));
        assert!(msg(5).contains("Joined") && msg(5).contains("not a date"), "{}", msg(5));
        assert_eq!(outcomes[4].problems[0].value, "pending");
        assert_eq!(outcomes[0].row, 2, "rows are numbered as in the file");
        // The row with no name is refused by the model itself, in its own words.
        assert!(msg(2).to_lowercase().contains("name") || msg(2).to_lowercase().contains("required"), "{}", msg(2));

        let records = all(&ctx, "contact").await?;
        assert_eq!(records.len(), 2, "only the good rows were written");
        let ann = records.iter().find(|r| r["name"] == "Ann").ok_or("no Ann")?;
        assert_eq!((ann["amount"].clone(), ann["status"].clone(), ann["joined"].clone(), ann["active"].clone()), (json!("1234.50"), json!("won"), json!("2026-03-01"), json!(true)));
        let bob = records.iter().find(|r| r["name"] == "Bob").ok_or("no Bob")?;
        assert_eq!((bob["status"].clone(), bob["amount"].clone()), (json!("new"), json!("10.00")), "the model's default, and a decimal made exact");
        assert!(outcomes[0].record_id.is_some() && outcomes[0].after.is_some());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn a_key_makes_a_second_run_change_nothing_and_a_changed_cell_an_update() -> TestResult {
        let Some(world) = World::new("imp_upsert").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let ctx = world.ctx(&models);
        let contact = &models[0];
        let first = table("Name,Email,Amount\nAnn,ann@x.com,10\nBob,bob@x.com,20\n");
        let mut mapping = mapping_for(&first, contact);
        mapping.key = vec!["email".into()];
        assert_eq!(actions(&run(&ctx, contact, &first, &mapping, false).await), ["create", "create"]);

        // The same file again: nothing to do, and nothing is written.
        let again = run(&ctx, contact, &first, &mapping, false).await;
        assert_eq!(actions(&again), ["unchanged", "unchanged"]);
        assert!(again.iter().all(|o| o.record_id.is_some()));
        assert_eq!(all(&ctx, "contact").await?.len(), 2);

        // A changed cell is an update that remembers what it replaced; a new key is a create.
        let next = table("Name,Email,Amount\nAnn,ann@x.com,10.00\nBob Marley,bob@x.com,25\nCy,cy@x.com,1\n");
        let outcomes = run(&ctx, contact, &next, &mapping, false).await;
        assert_eq!(actions(&outcomes), ["unchanged", "update", "create"], "10 and 10.00 are the same amount");
        let update = &outcomes[1];
        assert_eq!(update.before.as_ref().map(|b| Value::Object(b.clone())), Some(json!({ "name": "Bob", "amount": "20.00" })));
        assert_eq!(update.after.as_ref().map(|a| Value::Object(a.clone())), Some(json!({ "name": "Bob Marley", "amount": "25.00" })));
        let records = all(&ctx, "contact").await?;
        assert_eq!(records.len(), 3);
        assert!(records.iter().any(|r| r["name"] == "Bob Marley" && r["amount"] == "25.00"));

        // A row without the key, and a key that matches two records, are errors.
        let odd = table("Name,Email,Amount\nNoKey,,1\n");
        let o = run(&ctx, contact, &odd, &mapping, false).await;
        assert!(o[0].message.clone().unwrap_or_default().contains("identifies the record"), "{o:#?}");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn defaults_fill_new_records_only_and_a_unique_value_is_refused_by_the_model() -> TestResult {
        let Some(world) = World::new("imp_defaults").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let ctx = world.ctx(&models);
        let contact = &models[0];
        let t = table("Name,Email\nAnn,ann@x.com\nAnn again,ann@x.com\n");
        let mut mapping = mapping_for(&t, contact);
        mapping.defaults.insert("status".into(), json!("won"));
        // No key: each row is a create, so the second hits the unique email.
        let outcomes = run(&ctx, contact, &t, &mapping, false).await;
        assert_eq!(actions(&outcomes), ["create", "error"], "{outcomes:#?}");
        assert!(outcomes[1].message.is_some());
        let records = all(&ctx, "contact").await?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["status"], "won", "the default");
        // With a key the same default does not overwrite what an update leaves alone.
        mapping.key = vec!["email".into()];
        let again = run(&ctx, contact, &table("Name,Email\nAnn,ann@x.com\n"), &mapping, false).await;
        assert_eq!(actions(&again), ["unchanged"]);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn a_link_can_be_found_by_name_and_says_when_it_cannot() -> TestResult {
        let Some(world) = World::new("imp_links").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let ctx = world.ctx(&models);
        let acme = kernel_command(&ctx, "db::create", json!({ "model": "company", "data": { "title": "Acme" } })).await?["data"]["id"].clone();
        for _ in 0..2 {
            kernel_command(&ctx, "db::create", json!({ "model": "company", "data": { "title": "Twin" } })).await?;
        }
        let contact = &models[0];
        let t = table("Name,Company\nAnn,Acme\nBob,Nowhere\nCy,Twin\nDi,\n");
        let mut mapping = mapping_for(&t, contact);
        mapping.link_by.insert("company".into(), "title".into());
        let outcomes = run(&ctx, contact, &t, &mapping, false).await;
        assert_eq!(actions(&outcomes), ["create", "error", "error", "create"], "{outcomes:#?}");
        assert!(outcomes[1].message.clone().unwrap_or_default().contains("Nowhere"));
        assert!(outcomes[2].message.clone().unwrap_or_default().contains("more than one"));
        let records = all(&ctx, "contact").await?;
        let ann = records.iter().find(|r| r["name"] == "Ann").ok_or("no Ann")?;
        assert_eq!(ann["company"], acme);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn a_dry_run_says_exactly_what_a_real_run_does_and_writes_nothing() -> TestResult {
        let Some(world) = World::new("imp_dry").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let real = world.ctx(&models);
        let dry = real.clone().with_dry_run();
        let contact = &models[0];
        kernel_command(&real, "db::create", json!({ "model": "contact", "data": { "name": "Ann", "email": "ann@x.com" } })).await?;
        let t = table("Name,Email,Status\nAnn B,ann@x.com,won\nBob,bob@x.com,new\nCy,cy@x.com,bogus\n,dup@x.com,new\n");
        let mut mapping = mapping_for(&t, contact);
        mapping.key = vec!["email".into()];

        let rehearsal = run(&dry, contact, &t, &mapping, false).await;
        let after_dry = all(&real, "contact").await?;
        assert_eq!(after_dry.len(), 1, "nothing was written");
        assert_eq!(after_dry[0]["name"], "Ann", "nor changed");
        let performance = run(&real, contact, &t, &mapping, false).await;
        assert_eq!(actions(&rehearsal), actions(&performance), "the same decisions");
        assert_eq!(actions(&performance), ["update", "create", "error", "error"]);
        let words = |o: &Outcome| o.message.clone().unwrap_or_default();
        for (a, b) in rehearsal.iter().zip(&performance) {
            assert_eq!(words(a), words(b), "row {}: the same words", a.row);
        }
        assert!(rehearsal[1].record_id.is_none(), "a rehearsed create has no record");
        assert!(performance[1].record_id.is_some());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
    async fn stop_on_error_ends_the_batch_at_the_first_failure() -> TestResult {
        let Some(world) = World::new("imp_stop").await? else {
            return Ok(());
        };
        let models = models();
        world.apply(&models).await?;
        let ctx = world.ctx(&models);
        let contact = &models[0];
        let t = table("Name,Status\nAnn,new\nBob,bogus\nCy,new\n");
        let mapping = mapping_for(&t, contact);
        let outcomes = run(&ctx, contact, &t, &mapping, true).await;
        assert_eq!(actions(&outcomes), ["create", "error"], "Cy is never reached");
        assert_eq!(all(&ctx, "contact").await?.len(), 1);
        let _ = Action::Create;
        Ok(())
    }
}
