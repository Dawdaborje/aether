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
use aether_core::websocket::NotificationHub;
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

async fn context(db: &Surreal<Client>, org: &str, actor: Actor, grants: &[(&str, &str, bool, bool)], caps: &[&str]) -> PluginHostContext {
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
                },
            )
        })
        .collect();
    PluginHostContext::new(
        "chat",
        caps.iter().map(|c| c.to_string()).collect::<HashSet<_>>(),
        models,
        {
            let session = db.clone();
            session.use_ns(NAMESPACE).use_db(org).await.expect("select org database");
            std::sync::Arc::new(session)
        },
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
    )
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
    ).await;

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

    let ctx = context(&db, &org, Actor::User("users:u1".into()), &[("message", "chat_message", true, true)], &["db::query", "db::mutate"]).await;
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
    let ctx = context(&db, &org, Actor::Visitor("visitors:v2".into()), &[("message", "chat_message", true, false)], &["db::query"]).await;

    let denied = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "x" } })).await;
    assert!(matches!(denied, Err(HostError::Capability(_))), "no db::mutate capability: {denied:?}");

    let ctx = context(&db, &org, Actor::Visitor("visitors:v2".into()), &[("message", "chat_message", true, false)], &["db::query", "db::mutate"]).await;
    let denied = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "x" } })).await;
    assert!(matches!(denied, Err(HostError::ModelPermission(_, "write"))), "{denied:?}");

    let denied = kernel_command(&ctx, "db::find", json!({ "model": "secret" })).await;
    assert!(matches!(denied, Err(HostError::ModelDenied(_))));

    assert!(audit_rows(&db, &org).await?.is_empty(), "refusals happen before any data access");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn kernel_tables_cannot_be_used_as_models_or_touched_by_raw_surql() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "reserved").await?;
    let ctx = context(
        &db,
        &org,
        Actor::User("users:u1".into()),
        &[("log", "data_access", true, true), ("v", "visitors", true, true)],
        &["db::query", "db::mutate", "db::surql"],
    ).await;

    for (model, command) in [("log", "db::find"), ("v", "db::find")] {
        let result = kernel_command(&ctx, command, json!({ "model": model })).await;
        assert!(matches!(result, Err(HostError::ReservedTable(_))), "{model}: {result:?}");
    }
    let result = kernel_command(&ctx, "db::surql", json!({ "query": "DELETE page_visits" })).await;
    assert!(matches!(result, Err(HostError::SurqlRejected(_))));
    let result = kernel_command(&ctx, "db::surql", json!({ "query": "SELECT 1; COMMIT TRANSACTION; DELETE data_access" })).await;
    assert!(matches!(result, Err(HostError::SurqlRejected(_))));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn raw_surql_is_recorded_with_its_statement_and_find_is_capped() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let org = fresh_org(&db, "raw").await?;
    let ctx = context(&db, &org, Actor::User("users:u1".into()), &[("message", "chat_message", true, true)], &["db::query", "db::mutate", "db::surql"]).await;

    for body in ["a", "b", "c"] {
        kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": body } })).await?;
    }
    let limited = kernel_command(&ctx, "db::find", json!({ "model": "message", "limit": 2 })).await?;
    assert_eq!(limited["data"].as_array().map(Vec::len), Some(2));

    kernel_command(&ctx, "db::surql", json!({ "query": "SELECT * FROM chat_message" })).await?;
    let rows = audit_rows(&db, &org).await?;
    let raw = rows.last().ok_or("no audit rows")?;
    assert_eq!(raw["operation"], "raw");
    assert_eq!(raw["statement"], "SELECT * FROM chat_message");
    assert_eq!(raw["actor_type"], "user");
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
