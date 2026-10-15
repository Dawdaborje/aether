//! Creating organizations against a real SurrealDB (see `host_db_audit.rs` for how to run).

use aether_core::app_dir::AppDir;
use aether_core::config_manager::models::MediaConfig;
use aether_core::media::build_media_backend;
use aether_core::org_admin::{
    FirstMember, OrganizationError, OrganizationRequest, StorageTarget, accept_account_invite,
    assign_user, can_manage_users, create_organization, create_user, grant_member_role,
    invite_member, is_org_developer, list_members, list_platform_users, set_developer_by_login,
    set_member_active, set_org_developer, soft_kick_member,
};
use aether_core::roles;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

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
    db.signin(Root {
        username: "root".into(),
        password: "root".into(),
    })
    .await?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let namespace = format!("aether_org_admin_{suffix}");
    aether_orm::migrate_core(&db, &namespace, "core").await?;
    let directory = std::env::temp_dir().join(format!("aether_org_admin_{suffix}"));
    let media = build_media_backend(&MediaConfig::default_for(&directory)).await?;
    Ok(Some(World {
        db,
        namespace,
        storage: StorageTarget {
            app_dir: AppDir::new(&directory),
            media,
        },
    }))
}

fn request<'a>(name: &'a str, member: FirstMember<'a>) -> OrganizationRequest<'a> {
    OrganizationRequest {
        name,
        db_name: None,
        member,
        description: None,
        parent_db_name: None,
    }
}

fn new_user<'a>(username: &'a str) -> FirstMember<'a> {
    FirstMember::User {
        username,
        email: "ada@acme.io",
        password: "correct horse",
    }
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
    let Some(world) = world().await? else {
        return Ok(());
    };
    let (db_name, storage) = create_organization(
        &world.db,
        &world.namespace,
        &request("Acme Test", new_user("ada")),
        &world.storage,
    )
    .await?;
    assert_eq!(db_name, "acme_test");
    assert!(
        storage.directory.exists(),
        "the organization's folder exists"
    );

    assert_eq!(
        count(
            &world,
            "SELECT * FROM org_databases WHERE db_name = 'acme_test';"
        )
        .await?,
        1
    );
    assert_eq!(count(&world, "SELECT * FROM organization_users;").await?, 1);

    // The organization's own database has the full schema, including later migrations.
    world.db.use_db("acme_test").await?;
    let mut response = world.db.query("INFO FOR DB;").await?.check()?;
    let info: Option<serde_json::Value> = response.take(0)?;
    let tables = info
        .as_ref()
        .and_then(|info| info.get("tables"))
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(tables.contains("notifications"), "tables: {tables}");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn refusals_leave_nothing_behind() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;

    // The same identifier again.
    let taken = create_organization(
        &world.db,
        &world.namespace,
        &request("acme", new_user("bob")),
        &world.storage,
    )
    .await;
    assert!(matches!(taken, Err(OrganizationError::AlreadyExists(name)) if name == "acme"));
    assert_eq!(
        count(&world, "SELECT * FROM users WHERE username = 'bob';").await?,
        0,
        "no user was made"
    );

    // An existing user who does not exist.
    let missing = create_organization(
        &world.db,
        &world.namespace,
        &request("Globex", FirstMember::Existing { login: "nobody" }),
        &world.storage,
    )
    .await;
    assert!(matches!(missing, Err(OrganizationError::UserNotFound(_))));
    assert_eq!(
        count(
            &world,
            "SELECT * FROM org_databases WHERE db_name = 'globex';"
        )
        .await?,
        0
    );

    // The kernel's own database name.
    let reserved = create_organization(
        &world.db,
        &world.namespace,
        &request("Core", new_user("eve")),
        &world.storage,
    )
    .await;
    assert!(matches!(
        reserved,
        Err(OrganizationError::InvalidDatabaseName(_))
    ));
    assert_eq!(count(&world, "SELECT * FROM org_databases;").await?, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn an_existing_user_can_be_the_first_member() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;
    let (db_name, _) = create_organization(
        &world.db,
        &world.namespace,
        &request("Globex", FirstMember::Existing { login: "ada" }),
        &world.storage,
    )
    .await?;
    assert_eq!(db_name, "globex");
    // One user, two memberships.
    assert_eq!(
        count(&world, "SELECT * FROM users WHERE username = 'ada';").await?,
        1
    );
    assert_eq!(count(&world, "SELECT * FROM organization_users;").await?, 2);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn owner_and_user_manager_may_manage_members_but_manager_cannot_grant_admin() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    let (db_name, _) = create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;
    create_user(
        &world.db,
        &world.namespace,
        "bob",
        "bob@acme.io",
        "correct horse",
    )
    .await?;
    assign_user(&world.db, &world.namespace, "bob", &db_name).await?;

    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let mut response = world
        .db
        .query("SELECT VALUE <string> id FROM users WHERE username = 'ada' LIMIT 1;")
        .await?
        .check()?;
    let ada: Vec<String> = response.take(0)?;
    let ada_id = ada.into_iter().next().ok_or("ada missing")?;
    let mut response = world
        .db
        .query("SELECT VALUE <string> id FROM users WHERE username = 'bob' LIMIT 1;")
        .await?
        .check()?;
    let bob: Vec<String> = response.take(0)?;
    let bob_id = bob.into_iter().next().ok_or("bob missing")?;

    assert!(
        can_manage_users(&world.db, &world.namespace, &db_name, &ada_id, false).await?,
        "owner can manage"
    );
    assert!(
        !can_manage_users(&world.db, &world.namespace, &db_name, &bob_id, false).await?,
        "plain member cannot"
    );
    assert!(
        can_manage_users(&world.db, &world.namespace, &db_name, &bob_id, true).await?,
        "developer can"
    );

    world.db.use_db(&db_name).await?;
    roles::ensure_system_roles(&world.db).await?;
    roles::grant_by_id(&world.db, &bob_id, roles::USER_MANAGER).await?;
    assert!(
        can_manage_users(&world.db, &world.namespace, &db_name, &bob_id, false).await?,
        "user_manager can manage"
    );

    let denied = grant_member_role(
        &world.db,
        &world.namespace,
        &db_name,
        &bob_id,
        roles::ORG_ADMIN,
        false,
    )
    .await;
    assert!(matches!(
        denied,
        Err(OrganizationError::CannotManageAdminRole)
    ));

    grant_member_role(
        &world.db,
        &world.namespace,
        &db_name,
        &bob_id,
        roles::ORG_ADMIN,
        true,
    )
    .await?;

    let members = list_members(&world.db, &world.namespace, &db_name).await?;
    assert_eq!(members.len(), 2);
    let platform = list_platform_users(&world.db, &world.namespace).await?;
    assert!(
        platform
            .iter()
            .any(|u| u.username.as_deref() == Some("ada"))
    );
    assert!(
        platform
            .iter()
            .any(|u| u.username.as_deref() == Some("bob"))
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn invite_deactivate_and_soft_kick_leave_rows() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    let (db_name, _) = create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;

    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let mut response = world
        .db
        .query("SELECT VALUE <string> id FROM users WHERE username = 'ada' LIMIT 1;")
        .await?
        .check()?;
    let ada: Vec<String> = response.take(0)?;
    let ada_id = ada.into_iter().next().ok_or("ada missing")?;

    let (username, password) =
        invite_member(&world.db, &world.namespace, &db_name, "cara@acme.io", None).await?;
    assert!(!password.is_empty(), "new invite gets a temporary password");
    assert!(username.starts_with("cara"));

    let members = list_members(&world.db, &world.namespace, &db_name).await?;
    assert_eq!(members.len(), 2);
    let cara = members
        .iter()
        .find(|m| m.email.as_deref() == Some("cara@acme.io"))
        .ok_or("cara missing")?;
    assert_eq!(cara.is_active, true);

    set_member_active(
        &world.db,
        &world.namespace,
        &db_name,
        &cara.user_id,
        false,
        &ada_id,
    )
    .await?;
    let members = list_members(&world.db, &world.namespace, &db_name).await?;
    let cara = members
        .iter()
        .find(|m| m.user_id == cara.user_id)
        .ok_or("cara after deactivate")?;
    assert_eq!(cara.is_active, false);

    soft_kick_member(
        &world.db,
        &world.namespace,
        &db_name,
        &cara.user_id,
        &ada_id,
        false,
    )
    .await?;
    let members = list_members(&world.db, &world.namespace, &db_name).await?;
    assert_eq!(members.len(), 1, "kicked member hidden from active list");

    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let links = count(
        &world,
        &format!(
            "SELECT * FROM organization_users WHERE user_id = '{}' AND deleted_at != NONE;",
            cara.user_id
        ),
    )
    .await?;
    assert_eq!(links, 1, "membership row soft-deleted, not removed");
    Ok(())
}

async fn password_hash(world: &World, email: &str) -> Result<String, Box<dyn std::error::Error>> {
    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let mut response = world
        .db
        .query("SELECT VALUE hashed_password FROM users WHERE email = $email LIMIT 1;")
        .bind(("email", email.to_string()))
        .await?
        .check()?;
    let hashes: Vec<String> = response.take(0)?;
    hashes
        .into_iter()
        .next()
        .ok_or_else(|| "missing password hash".into())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn existing_account_keeps_its_password_and_an_invite_creates_once() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    let (db_name, _) = create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;

    let before = password_hash(&world, "ada@acme.io").await?;
    let (username, password) =
        invite_member(&world.db, &world.namespace, &db_name, "ada@acme.io", None).await?;
    assert_eq!(username, "ada");
    assert!(
        password.is_empty(),
        "an existing account is not given a new password"
    );
    assert_eq!(password_hash(&world, "ada@acme.io").await?, before);

    let user_id = accept_account_invite(
        &world.db,
        &world.namespace,
        &db_name,
        "erin@acme.io",
        Some("erin"),
        Some("Erin Example"),
        "correct horse",
    )
    .await?;
    assert!(user_id.starts_with("users:"));
    let created = password_hash(&world, "erin@acme.io").await?;
    let again = accept_account_invite(
        &world.db,
        &world.namespace,
        &db_name,
        "erin@acme.io",
        None,
        None,
        "a different passphrase",
    )
    .await;
    assert!(matches!(
        again,
        Err(OrganizationError::AccountAlreadyExists)
    ));
    assert_eq!(password_hash(&world, "erin@acme.io").await?, created);

    world.db.use_ns(&world.namespace).await?;
    world.db.use_db(&db_name).await?;
    let mut response = world
        .db
        .query("SELECT display_name FROM org_users WHERE core_user_id = $user LIMIT 1;")
        .bind(("user", user_id))
        .await?
        .check()?;
    let rows: Vec<serde_json::Value> = response.take(0)?;
    assert_eq!(
        rows.first()
            .and_then(|row| row.get("display_name"))
            .and_then(|name| name.as_str()),
        Some("Erin Example")
    );
    Ok(())
}

async fn user_id(world: &World, username: &str) -> Result<String, Box<dyn std::error::Error>> {
    world.db.use_ns(&world.namespace).await?;
    world.db.use_db("core").await?;
    let mut response = world
        .db
        .query("SELECT VALUE <string> id FROM users WHERE username = $name LIMIT 1;")
        .bind(("name", username.to_string()))
        .await?
        .check()?;
    let found: Vec<String> = response.take(0)?;
    Ok(found.into_iter().next().ok_or("user missing")?)
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_developer_of_one_organization_is_not_one_of_another() -> TestResult {
    let Some(world) = world().await? else {
        return Ok(());
    };
    let (acme, _) = create_organization(
        &world.db,
        &world.namespace,
        &request("Acme", new_user("ada")),
        &world.storage,
    )
    .await?;
    let (globex, _) = create_organization(
        &world.db,
        &world.namespace,
        &request(
            "Globex",
            FirstMember::User {
                username: "gus",
                email: "gus@globex.io",
                password: "correct horse",
            },
        ),
        &world.storage,
    )
    .await?;
    create_user(
        &world.db,
        &world.namespace,
        "dev",
        "dev@acme.io",
        "correct horse",
    )
    .await?;
    assign_user(&world.db, &world.namespace, "dev", &acme).await?;
    assign_user(&world.db, &world.namespace, "dev", &globex).await?;
    let dev = user_id(&world, "dev").await?;
    let ns = &world.namespace;

    assert!(
        !is_org_developer(&world.db, ns, &acme, &dev, false).await?,
        "nobody is one by default"
    );
    set_org_developer(&world.db, ns, &acme, &dev, true).await?;
    assert!(
        is_org_developer(&world.db, ns, &acme, &dev, false).await?,
        "developer of acme"
    );
    assert!(
        !is_org_developer(&world.db, ns, &globex, &dev, false).await?,
        "but not of globex"
    );
    assert!(
        is_org_developer(&world.db, ns, &globex, &dev, true).await?,
        "unrestricted: everywhere"
    );

    let members = list_members(&world.db, ns, &acme).await?;
    let row = members
        .iter()
        .find(|m| m.user_id == dev)
        .ok_or("dev not listed")?;
    assert!(row.is_developer, "the member list shows it");

    let gus = user_id(&world, "gus").await?;
    let outsider = set_org_developer(&world.db, ns, &acme, &gus, true).await;
    assert!(
        matches!(outsider, Err(OrganizationError::NotAMember(_, _))),
        "only members"
    );

    let ada = user_id(&world, "ada").await?;
    soft_kick_member(&world.db, ns, &acme, &dev, &ada, false).await?;
    assert!(
        !is_org_developer(&world.db, ns, &acme, &dev, false).await?,
        "leaving ends it"
    );

    set_developer_by_login(&world.db, ns, "dev", Some(&globex), true).await?;
    assert!(
        is_org_developer(&world.db, ns, &globex, &dev, false).await?,
        "granted by login"
    );
    set_developer_by_login(&world.db, ns, "dev", Some(&globex), false).await?;
    assert!(
        !is_org_developer(&world.db, ns, &globex, &dev, false).await?,
        "revoked by login"
    );

    set_developer_by_login(&world.db, ns, "dev", None, true).await?;
    world.db.use_ns(ns).await?;
    world.db.use_db("core").await?;
    let mut response = world
        .db
        .query("SELECT VALUE is_super_user FROM users WHERE username = 'dev' LIMIT 1;")
        .await?
        .check()?;
    let flags: Vec<bool> = response.take(0)?;
    assert_eq!(flags, vec![true], "no --org: unrestricted");
    Ok(())
}
