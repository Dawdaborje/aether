//! Creating organizations against a real SurrealDB (see `host_db_audit.rs` for how to run).

use aether_core::app_dir::AppDir;
use aether_core::config_manager::models::MediaConfig;
use aether_core::media::build_media_backend;
use aether_core::org_admin::{
    FirstMember, OrganizationError, OrganizationRequest, StorageTarget, create_organization,
};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct World {
    db: Surreal<Client>,
    namespace: String,
    storage: StorageTarget,
}

async fn world() -> Result<Option<World>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let namespace = format!("aether_org_admin_{suffix}");
    aether_orm::migrate_core(&db, &namespace, "core").await?;
    let directory = std::env::temp_dir().join(format!("aether_org_admin_{suffix}"));
    let media = build_media_backend(&MediaConfig::default_for(&directory)).await?;
    Ok(Some(World { db, namespace, storage: StorageTarget { app_dir: AppDir::new(&directory), media } }))
}

fn request<'a>(name: &'a str, member: FirstMember<'a>) -> OrganizationRequest<'a> {
    OrganizationRequest { name, db_name: None, member }
}

fn new_user<'a>(username: &'a str) -> FirstMember<'a> {
    FirstMember::User { username, email: "ada@acme.io", password: "correct horse" }
}

async fn count(world: &World, query: &str) -> Result<usize, Box<dyn std::error::Error>> {
    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let mut response = world.db.query(query).await?.check()?;
    let rows: Vec<serde_json::Value> = response.take(0)?;
    Ok(rows.len())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn creates_the_organization_with_its_schema_user_and_storage() -> TestResult {
    let Some(world) = world().await? else { return Ok(()) };
    let (db_name, storage) =
        create_organization(&world.db, &world.namespace, &request("Acme Test", new_user("ada")), &world.storage).await?;
    assert_eq!(db_name, "acme_test");
    assert!(storage.directory.exists(), "the organization's folder exists");

    assert_eq!(count(&world, "SELECT * FROM org_databases WHERE db_name = 'acme_test';").await?, 1);
    assert_eq!(count(&world, "SELECT * FROM organization_users;").await?, 1);

    // The organization's own database has the full schema, including later migrations.
    world.db.use_db("acme_test").await?;
    let mut response = world.db.query("INFO FOR DB;").await?.check()?;
    let info: Option<serde_json::Value> = response.take(0)?;
    let tables = info.as_ref().and_then(|info| info.get("tables")).map(ToString::to_string).unwrap_or_default();
    assert!(tables.contains("notifications"), "tables: {tables}");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn refusals_leave_nothing_behind() -> TestResult {
    let Some(world) = world().await? else { return Ok(()) };
    create_organization(&world.db, &world.namespace, &request("Acme", new_user("ada")), &world.storage).await?;

    // The same identifier again.
    let taken = create_organization(&world.db, &world.namespace, &request("acme", new_user("bob")), &world.storage).await;
    assert!(matches!(taken, Err(OrganizationError::AlreadyExists(name)) if name == "acme"));
    assert_eq!(count(&world, "SELECT * FROM users WHERE username = 'bob';").await?, 0, "no user was made");

    // An existing user who does not exist.
    let missing = create_organization(
        &world.db, &world.namespace,
        &request("Globex", FirstMember::Existing { login: "nobody" }), &world.storage,
    ).await;
    assert!(matches!(missing, Err(OrganizationError::UserNotFound(_))));
    assert_eq!(count(&world, "SELECT * FROM org_databases WHERE db_name = 'globex';").await?, 0);

    // The kernel's own database name.
    let reserved = create_organization(&world.db, &world.namespace, &request("Core", new_user("eve")), &world.storage).await;
    assert!(matches!(reserved, Err(OrganizationError::InvalidDatabaseName(_))));
    assert_eq!(count(&world, "SELECT * FROM org_databases;").await?, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn an_existing_user_can_be_the_first_member() -> TestResult {
    let Some(world) = world().await? else { return Ok(()) };
    create_organization(&world.db, &world.namespace, &request("Acme", new_user("ada")), &world.storage).await?;
    let (db_name, _) = create_organization(
        &world.db, &world.namespace,
        &request("Globex", FirstMember::Existing { login: "ada" }), &world.storage,
    ).await?;
    assert_eq!(db_name, "globex");
    // One user, two memberships.
    assert_eq!(count(&world, "SELECT * FROM users WHERE username = 'ada';").await?, 1);
    assert_eq!(count(&world, "SELECT * FROM organization_users;").await?, 2);
    Ok(())
}
