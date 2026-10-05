//! Host database commands against a real SurrealDB.
//!
//! Needs a server, so the tests are `#[ignore]`d. Run them with:
//!
//! ```text
//! surreal start memory --user root --pass root --bind 127.0.0.1:18000
//! AETHER_TEST_DB=127.0.0.1:18000 cargo test -p aether_core --test host_db_audit -- --ignored
//! ```

use std::collections::{HashMap, HashSet};

use aether_core::access::audit::{Actor, AuditContext};
use aether_core::kernel::{CallInfo, DbScope, HostError, ModelGrant, PluginHostContext, kernel_command};
use aether_core::notifications::NotificationHub;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_host_test";

async fn connect() -> Result<Option<Surreal<Client>>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    Ok(Some(db))
}

/// A fresh organization database with the real org migrations applied.
async fn fresh_org(db: &Surreal<Client>, label: &str) -> Result<String, Box<dyn std::error::Error>> {
    let unique: u32 = rand_suffix();
    let name = format!("org_{label}_{unique}");
    aether_orm::migrate_org(db, NAMESPACE, &name).await?;
    Ok(name)
}

fn rand_suffix() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(7)
}

async fn context(db: &Surreal<Client>, org: &str, actor: Actor, grants: &[(&str, &str, bool, bool)], caps: &[&str]) -> Result<PluginHostContext, surrealdb::Error> {
    let models: HashMap<String, ModelGrant> = grants
        .iter()
        .map(|(name, table, read, write)| {
            (
                name.to_string(),
                ModelGrant {
                    name: name.to_string(),
                    table: table.to_string(),
                    can_read: *read,
                    can_write: *write,
                    schema: None,
                    rules: None,
                },
            )
        })
        .collect();
    let session = db.clone();
    session.use_ns(NAMESPACE).use_db(org).await?;
    Ok(PluginHostContext::new(
        "chat",
        caps.iter().map(|c| c.to_string()).collect::<HashSet<_>>(),
        models,
        std::sync::Arc::new(session),
        DbScope::new(NAMESPACE, org),
        NotificationHub::default(),
        CallInfo::new(
            AuditContext {
                actor,
                request_id: "req-1".into(),
                ip: Some("203.0.113.0".into()),
                user_agent: None,
            },
            "post_message",
        ),
    ))
}

async fn audit_rows(db: &Surreal<Client>, org: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    db.use_ns(NAMESPACE).await?;
    db.use_db(org).await?;
    let mut response = db
        .query("SELECT actor_type, actor_id, plugin, function_name, model, table_name, operation, <string> record_ids AS ids, record_count, ip, statement, date_created FROM data_access ORDER BY date_created;")
        .await?
        .check()?;
    Ok(response.take(0)?)
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn every_access_is_recorded_with_record_ids() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "audit").await?;
    let ctx = context(
        &db,
        &org,
        Actor::Visitor("visitors:v1".into()),
        &[("message", "chat_message", true, true)],
        &["db::query", "db::mutate"],
    ).await?;

    let created = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "hi" } })).await?;
    let id = created["data"]["id"].as_str().ok_or("create returned no id")?.to_string();
    assert!(id.starts_with("chat_message:"), "{id}");

    let found = kernel_command(&ctx, "db::find", json!({ "model": "message", "filter": { "body": "hi" } })).await?;
    assert_eq!(found["data"].as_array().map(Vec::len), Some(1));

    let fetched = kernel_command(&ctx, "db::get", json!({ "model": "message", "id": id })).await?;
    assert_eq!(fetched["data"]["body"], "hi");

    kernel_command(&ctx, "db::update", json!({ "model": "message", "id": id, "data": { "body": "edited" } })).await?;
    kernel_command(&ctx, "db::delete", json!({ "model": "message", "id": id })).await?;

    let rows = audit_rows(&db, &org).await?;
    let operations: Vec<_> = rows.iter().map(|row| row["operation"].as_str().unwrap_or("")).collect();
    assert_eq!(operations, ["create", "read", "read", "update", "delete"]);

    for row in &rows {
        assert_eq!(row["actor_type"], "visitor");
        assert_eq!(row["actor_id"], "visitors:v1");
        assert_eq!(row["plugin"], "chat");
        assert_eq!(row["function_name"], "post_message");
        assert_eq!(row["model"], "message");
        assert_eq!(row["table_name"], "chat_message");
        assert_eq!(row["ip"], "203.0.113.0");
        assert!(row["record_count"].as_i64().unwrap_or(-1) >= 1, "{row}");
        let ids = row["ids"].as_str().unwrap_or("");
        assert!(ids.contains(&id.replace("chat_message:", "")), "ids {ids} should name {id}");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_failed_audit_write_rolls_back_the_change() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "failclosed").await?;
    // Make the audit insert fail for this request id.
    db.use_ns(NAMESPACE).await?;
    db.use_db(&org).await?;
    db.query("DEFINE FIELD OVERWRITE request_id ON TABLE data_access TYPE string ASSERT $value != 'req-1';")
        .await?
        .check()?;

    let ctx = context(&db, &org, Actor::User("users:u1".into()), &[("message", "chat_message", true, true)], &["db::query", "db::mutate"]).await?;
    let result = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "should not persist" } })).await;
    assert!(matches!(result, Err(HostError::Db(_))), "audit failure must fail the command: {result:?}");

    // A rolled-back CREATE never even defines the table, so "does not exist"
    // is as good as "no rows".
    let rows: Vec<Value> = match db.query("SELECT * FROM chat_message;").await?.check() {
        Ok(mut response) => response.take(0)?,
        Err(error) if error.to_string().contains("does not exist") => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    assert!(rows.is_empty(), "the data change must not persist without its audit row: {rows:?}");

    let reads = kernel_command(&ctx, "db::find", json!({ "model": "message" })).await;
    assert!(reads.is_err(), "reads fail closed too");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn read_only_grants_cannot_write_and_leave_no_data_access_row() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "readonly").await?;
    let ctx = context(&db, &org, Actor::Visitor("visitors:v2".into()), &[("message", "chat_message", true, false)], &["db::query"]).await?;

    let denied = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "x" } })).await;
    assert!(matches!(denied, Err(HostError::Capability(_))), "no db::mutate capability: {denied:?}");

    let ctx = context(&db, &org, Actor::Visitor("visitors:v2".into()), &[("message", "chat_message", true, false)], &["db::query", "db::mutate"]).await?;
    let denied = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "x" } })).await;
    assert!(matches!(denied, Err(HostError::ModelPermission(_, "write"))), "{denied:?}");

    let denied = kernel_command(&ctx, "db::find", json!({ "model": "secret" })).await;
    assert!(matches!(denied, Err(HostError::ModelDenied(_))));

    assert!(audit_rows(&db, &org).await?.is_empty(), "refusals happen before any data access");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn kernel_tables_cannot_be_used_as_models() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "reserved").await?;
    let ctx = context(
        &db,
        &org,
        Actor::User("users:u1".into()),
        &[("log", "data_access", true, true), ("v", "visitors", true, true)],
        &["db::query", "db::mutate"],
    ).await?;

    for (model, command, payload) in [
        ("log", "db::find", json!({ "model": "log" })),
        ("v", "db::find", json!({ "model": "v" })),
        ("v", "db::create", json!({ "model": "v", "data": { "token_hash": "x" } })),
        ("log", "db::delete", json!({ "model": "log", "id": "any" })),
    ] {
        let result = kernel_command(&ctx, command, payload).await;
        assert!(matches!(result, Err(HostError::ReservedTable(_))), "{model} {command}: {result:?}");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn find_is_capped_and_cannot_be_used_to_inject_queries() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "find").await?;
    let ctx = context(&db, &org, Actor::User("users:u1".into()), &[("message", "chat_message", true, true)], &["db::query", "db::mutate"]).await?;

    for body in ["a", "b", "c"] {
        kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": body } })).await?;
    }
    let limited = kernel_command(&ctx, "db::find", json!({ "model": "message", "limit": 2 })).await?;
    assert_eq!(limited["data"].as_array().map(Vec::len), Some(2));

    // A value is only ever a value, however much it looks like a query...
    let tricky = kernel_command(&ctx, "db::find", json!({ "model": "message", "filter": { "body": "a' OR true --" } })).await?;
    assert_eq!(tricky["data"].as_array().map(Vec::len), Some(0));
    // ...and a field name must be a plain identifier.
    for field in ["body = 1 OR true", "body; DELETE chat_message", "a.b"] {
        let result = kernel_command(&ctx, "db::find", json!({ "model": "message", "filter": { field: 1 } })).await;
        assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{field}: {result:?}");
    }
    let order = kernel_command(&ctx, "db::find", json!({ "model": "message", "order": "body; DELETE chat_message" })).await;
    assert!(matches!(order, Err(HostError::InvalidPayload(_))));
    // Nothing was deleted by any of that.
    let all = kernel_command(&ctx, "db::find", json!({ "model": "message" })).await?;
    assert_eq!(all["data"].as_array().map(Vec::len), Some(3));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn retention_purges_only_rows_older_than_the_window() -> TestResult {
    use aether_core::access::audit::purge_expired;

    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "retention").await?;
    let core = format!("core_{}", rand_suffix());

    db.use_ns(NAMESPACE).await?;
    db.use_db(&core).await?;
    db.query("CREATE org_databases SET db_name = $org;")
        .bind(("org", org.clone()))
        .await?
        .check()?;

    db.use_db(&org).await?;
    db.query(
        r#"
        CREATE page_visits SET request_id='old', actor_type='visitor', path='/', method='GET', status=200, date_created = time::now() - 40d;
        CREATE page_visits SET request_id='new', actor_type='visitor', path='/', method='GET', status=200;
        CREATE plugin_calls SET request_id='old', actor_type='visitor', plugin='p', function_name='f', status=200, date_created = time::now() - 40d;
        CREATE data_access SET request_id='old', actor_type='visitor', plugin='p', function_name='f', operation='read', date_created = time::now() - 40d;
        CREATE data_access SET request_id='new', actor_type='visitor', plugin='p', function_name='f', operation='read';
        CREATE visitors SET token_hash='stale', last_seen = time::now() - 40d;
        CREATE visitors SET token_hash='fresh';
        "#,
    )
    .await?
    .check()?;

    let report = purge_expired(&db, NAMESPACE, &core, 30).await?;
    assert_eq!(
        (report.organizations, report.page_visits, report.plugin_calls, report.data_access, report.visitors),
        (1, 1, 1, 1, 1)
    );

    db.use_db(&org).await?;
    let mut response = db
        .query("SELECT request_id FROM page_visits; SELECT token_hash FROM visitors;")
        .await?
        .check()?;
    let visits: Vec<Value> = response.take(0)?;
    let visitors: Vec<Value> = response.take(1)?;
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0]["request_id"], "new");
    assert_eq!(visitors.len(), 1);
    assert_eq!(visitors[0]["token_hash"], "fresh");

    let again = purge_expired(&db, NAMESPACE, &core, 30).await?;
    assert_eq!(
        (again.page_visits, again.plugin_calls, again.data_access, again.visitors),
        (0, 0, 0, 0)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_transaction_applies_every_write_or_none() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "txn").await?;
    let ctx = context(
        &db,
        &org,
        Actor::Visitor("visitors:v1".into()),
        &[("message", "chat_message", true, true)],
        &["db::query", "db::mutate", "db::transaction"],
    )
    .await?;

    let first = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "one", "n": 1 } })).await?;
    let first_id = first["data"]["id"].as_str().ok_or("no id")?.to_string();

    // Two creates, an update and an increment, together.
    let done = kernel_command(
        &ctx,
        "db::transaction",
        json!({ "ops": [
            { "op": "create", "model": "message", "data": { "body": "two" } },
            { "op": "create", "model": "message", "data": { "body": "three" } },
            { "op": "update", "model": "message", "id": first_id, "data": { "body": "edited" } },
            { "op": "increment", "model": "message", "id": first_id, "field": "n", "by": 4 },
        ] }),
    )
    .await?;
    let results = done["data"].as_array().ok_or("no results")?;
    assert_eq!(results.len(), 4);
    assert_eq!(results[0]["body"], "two");
    assert_eq!(results[1]["body"], "three");
    assert_eq!(results[3]["n"], 5);
    let all = kernel_command(&ctx, "db::find", json!({ "model": "message" })).await?;
    assert_eq!(all["data"].as_array().map(Vec::len), Some(3));
    let edited = kernel_command(&ctx, "db::get", json!({ "model": "message", "id": first_id })).await?;
    assert_eq!(edited["data"]["body"], "edited");

    // One audit row per write (the first create, then the four of the transaction).
    let rows = audit_rows(&db, &org).await?;
    let writes: Vec<_> = rows.iter().filter(|row| row["operation"] != "read").map(|row| row["operation"].as_str().unwrap_or("")).collect();
    assert_eq!(writes, ["create", "create", "create", "update", "update"]);

    // A write that fails in the database (adding to text) undoes the ones before it.
    let failed = kernel_command(
        &ctx,
        "db::transaction",
        json!({ "ops": [
            { "op": "create", "model": "message", "data": { "body": "ghost" } },
            { "op": "increment", "model": "message", "id": first_id, "field": "body", "by": 1 },
        ] }),
    )
    .await;
    assert!(failed.is_err(), "adding to text must fail");
    let after = kernel_command(&ctx, "db::find", json!({ "model": "message", "filter": { "body": "ghost" } })).await?;
    assert_eq!(after["data"].as_array().map(Vec::len), Some(0), "the first write must have been rolled back");

    // Refused before anything runs: no capability, bad op, no write grant, too many.
    let reader = context(&db, &org, Actor::Anonymous, &[("message", "chat_message", true, false)], &["db::query", "db::mutate", "db::transaction"]).await?;
    let denied = kernel_command(&reader, "db::transaction", json!({ "ops": [{ "op": "create", "model": "message", "data": { "body": "x" } }] })).await;
    assert!(matches!(denied, Err(HostError::ModelPermission(_, _))), "{denied:?}");
    let bad = kernel_command(&ctx, "db::transaction", json!({ "ops": [{ "op": "explode" }] })).await;
    assert!(matches!(bad, Err(HostError::InvalidPayload(_))), "{bad:?}");
    let many: Vec<Value> = (0..51).map(|_| json!({ "op": "delete", "model": "message", "id": "x" })).collect();
    assert!(kernel_command(&ctx, "db::transaction", json!({ "ops": many })).await.is_err());
    let no_cap = context(&db, &org, Actor::Anonymous, &[("message", "chat_message", true, true)], &["db::mutate"]).await?;
    assert!(matches!(
        kernel_command(&no_cap, "db::transaction", json!({ "ops": [{ "op": "delete", "model": "message", "id": "x" }] })).await,
        Err(HostError::Capability(_))
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn filters_counts_and_aggregates_read_what_they_say() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "query").await?;
    let ctx = context(&db, &org, Actor::User("users:u1".into()), &[("staff", "hr_staff", true, true)], &["db::query", "db::mutate"]).await?;

    for (name, dept, pay) in [("Ann", "it", 10), ("Bob", "it", 30), ("Cy", "hr", 20), ("Di", "ops", 40)] {
        kernel_command(&ctx, "db::create", json!({ "model": "staff", "data": { "name": name, "dept": dept, "pay": pay } })).await?;
    }
    // A record with no department at all.
    kernel_command(&ctx, "db::create", json!({ "model": "staff", "data": { "name": "Ed", "pay": 5 } })).await?;

    let names = |reply: Value| -> Vec<String> {
        let mut found: Vec<String> = reply["data"]
            .as_array()
            .map(|rows| rows.iter().filter_map(|row| row["name"].as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        found.sort();
        found
    };
    let find = |filter: Value| kernel_command(&ctx, "db::find", json!({ "model": "staff", "filter": filter }));

    assert_eq!(names(find(json!({ "pay": { "gte": 20, "lt": 40 } })).await?), ["Bob", "Cy"]);
    assert_eq!(names(find(json!({ "dept": { "in": ["hr", "ops"] } })).await?), ["Cy", "Di"]);
    // A record with no value is "not in" any list, so Ed is here too.
    assert_eq!(names(find(json!({ "dept": { "nin": ["it"] } })).await?), ["Cy", "Di", "Ed"]);
    assert_eq!(names(find(json!({ "dept": { "null": true } })).await?), ["Ed"]);
    assert_eq!(names(find(json!({ "or": [{ "name": "Ann" }, { "pay": { "gt": 35 } }] })).await?), ["Ann", "Di"]);
    assert_eq!(names(find(json!({ "not": { "dept": "it" } })).await?), ["Cy", "Di", "Ed"]);
    assert_eq!(names(find(json!({ "name": { "like": "AN" } })).await?), ["Ann"]);

    let count = kernel_command(&ctx, "db::count", json!({ "model": "staff", "filter": { "dept": "it" } })).await?;
    assert_eq!(count["data"], json!(2));
    let none = kernel_command(&ctx, "db::count", json!({ "model": "staff", "filter": { "dept": "nowhere" } })).await?;
    assert_eq!(none["data"], json!(0));

    let by_dept = kernel_command(
        &ctx,
        "db::aggregate",
        json!({ "model": "staff", "filter": { "dept": { "null": false } }, "group_by": ["dept"],
                "aggs": { "n": "count", "total": { "sum": "pay" }, "mean": { "avg": "pay" }, "top": { "max": "pay" } } }),
    )
    .await?;
    let mut groups = by_dept["data"].as_array().cloned().unwrap_or_default();
    groups.sort_by_key(|g| g["dept"].as_str().map(str::to_string));
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[1]["dept"], json!("it"));
    assert_eq!(groups[1]["n"], json!(2));
    assert_eq!(groups[1]["total"], json!(40));
    assert_eq!(groups[1]["mean"].as_f64(), Some(20.0));
    assert_eq!(groups[1]["top"], json!(30));

    let whole = kernel_command(&ctx, "db::aggregate", json!({ "model": "staff", "aggs": { "n": "count" } })).await?;
    assert_eq!(whole["data"], json!([{ "n": 5 }]));

    // Bad requests are refused, and nothing is deleted by them.
    for bad in [json!({ "name; DELETE hr_staff": 1 }), json!({ "pay": { "bogus": 1 } })] {
        let result = find(bad).await;
        assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{result:?}");
    }
    let rows = audit_rows(&db, &org).await?;
    assert!(rows.iter().any(|row| row["operation"] == "read" && row["record_count"] == json!(3)), "aggregate is audited: {rows:?}");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn decimals_add_up_exactly_in_the_database() -> TestResult {
    use aether_core::data_model::{ModelDef, ModelSchema, sync_ids};
    use std::sync::Arc;

    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "decimal").await?;
    let mut model: ModelDef = serde_json::from_value(json!({
        "name": "pay", "fields": [
            { "name": "who", "type": "string" },
            { "name": "amount", "type": "decimal" },
            { "name": "balance", "type": "decimal", "scale": 1 }
        ]
    }))?;
    sync_ids(&mut model);
    let schema = ModelSchema::new(&model, std::slice::from_ref(&model)).ok_or("no schema")?;
    let table = schema.table.clone();
    // The columns exist as the planner would define them.
    let applied = aether_core::data_model::apply::plan("hr", &model, None, 0);
    db.use_ns(NAMESPACE).use_db(&org).await?;
    aether_core::data_model::apply::apply_plans(&db, &[(applied, None)]).await?;

    let mut ctx = context(&db, &org, Actor::User("users:u1".into()), &[("pay", &table, true, true)], &["db::query", "db::mutate"]).await?;
    if let Some(grant) = ctx.models.get_mut("pay") {
        grant.schema = Some(Arc::new(schema));
    }
    // 0.10 + 0.20 is exact, unlike a float.
    for amount in ["0.10", "0.20", "10.00"] {
        kernel_command(&ctx, "db::create", json!({ "model": "pay", "data": { "who": "a", "amount": amount } })).await?;
    }
    let sum = kernel_command(&ctx, "db::aggregate", json!({ "model": "pay", "aggs": { "total": { "sum": "amount" }, "top": { "max": "amount" } } })).await?;
    assert_eq!(sum["data"], json!([{ "total": "10.30", "top": "10.00" }]));
    let found = kernel_command(&ctx, "db::find", json!({ "model": "pay", "filter": { "amount": { "gt": "0.15", "lt": "5" } }, "order": "-amount" })).await?;
    assert_eq!(found["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(found["data"][0]["amount"], json!("0.20"));
    let counter = kernel_command(&ctx, "db::create", json!({ "model": "pay", "data": { "who": "leave", "balance": "20.0" } })).await?;
    let id = counter["data"]["id"].as_str().ok_or("no id")?.to_string();
    let after = kernel_command(&ctx, "db::increment", json!({ "model": "pay", "id": id, "field": "balance", "by": "-1.5" })).await?;
    assert_eq!(after["data"]["balance"], json!("18.5"));
    let too_fine = kernel_command(&ctx, "db::create", json!({ "model": "pay", "data": { "amount": "0.001" } })).await;
    assert!(matches!(too_fine, Err(HostError::InvalidPayload(_))), "{too_fine:?}");
    Ok(())
}
