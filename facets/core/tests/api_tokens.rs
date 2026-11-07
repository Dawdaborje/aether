//! API tokens against a real SurrealDB (see `host_db_audit.rs` for how to run): making, finding,
//! expiring and revoking them.

use aether_core::access::api_token::{self, NewToken, Scopes, TokenError};
use serde_json::{Value, json};
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_tokens_test";

async fn connect() -> Result<Option<Surreal<Client>>, Box<dyn std::error::Error>> {
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
    Ok(Some(db))
}

/// Core databases are made one at a time: two at once in one namespace can make SurrealDB refuse one
/// with a write conflict, which has nothing to do with what is tested here.
static SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A fresh core database with the real migrations and two people in it.
async fn fresh_core(db: &Surreal<Client>) -> Result<(), Box<dyn std::error::Error>> {
    let _one_at_a_time = SETUP.lock().await;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos();
    let name = format!("core_{unique}");
    aether_orm::migrate_core(db, NAMESPACE, &name).await?;
    db.use_ns(NAMESPACE).use_db(&name).await?;
    db.query(
        "CREATE users:ann SET username = 'ann', is_active = true; \
         CREATE users:bob SET username = 'bob', is_active = true, is_super_user = true;",
    )
    .await?
    .check()?;
    Ok(())
}

fn scopes() -> Scopes {
    Scopes {
        plugins: vec!["crm".into()],
        functions: vec![],
        models: vec!["crm.lead:read".into()],
    }
}

fn new<'a>(user: &'a str, name: &'a str, scopes: &'a Scopes, days: Option<u32>) -> NewToken<'a> {
    NewToken {
        user,
        org_db: "org_acme",
        name,
        scopes,
        expires_days: days,
    }
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_token_is_made_once_and_found_by_its_secret() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    fresh_core(&db).await?;
    let s = scopes();
    let (id, raw) = api_token::create(&db, new("users:ann", "nightly sync", &s, Some(30))).await?;
    assert!(raw.starts_with("aet_"));

    let found = api_token::lookup(&db, &raw).await?.ok_or("not found")?;
    assert_eq!((found.id.as_str(), found.user.as_str(), found.org_db.as_str()), (id.as_str(), "users:ann", "org_acme"));
    assert_eq!(found.scopes, s);
    assert!(found.user_active && !found.user_is_super);
    assert!(api_token::lookup(&db, "aet_not-a-real-token").await?.is_none());

    // Only the hash is stored.
    let mut response = db.query("SELECT * FROM api_tokens;").await?.check()?;
    let rows: Vec<Value> = response.take(0)?;
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].to_string().contains(&raw), "the token itself is not kept");
    assert_eq!(rows[0]["token_hash"], api_token::hash(&raw));
    assert_eq!(rows[0]["prefix"], api_token::display_prefix(&raw));

    // A person sees their tokens, with no secret in them.
    let listed = api_token::list(&db, "users:ann", "org_acme").await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "nightly sync");
    assert!(!serde_json::to_string(&listed)?.contains(&raw));
    assert!(api_token::list(&db, "users:bob", "org_acme").await?.is_empty(), "not someone else's");
    assert!(api_token::list(&db, "users:ann", "org_other").await?.is_empty(), "not another organization's");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_token_stops_working_when_revoked_expired_or_its_owner_is_deactivated() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    fresh_core(&db).await?;
    let s = scopes();

    // Revoked: at once on this server.
    let (id, raw) = api_token::create(&db, new("users:ann", "a", &s, None)).await?;
    assert!(api_token::lookup(&db, &raw).await?.is_some());
    assert!(!api_token::revoke(&db, &id, "users:bob", "org_acme").await?, "not bob's to revoke");
    assert!(api_token::lookup(&db, &raw).await?.is_some(), "still works");
    assert!(api_token::revoke(&db, &id, "users:ann", "org_acme").await?);
    assert!(api_token::lookup(&db, &raw).await?.is_none());
    assert!(!api_token::revoke(&db, &id, "users:ann", "org_acme").await?, "already revoked");
    let listed = api_token::list(&db, "users:ann", "org_acme").await?;
    assert!(listed[0].revoked_at.is_string(), "kept, marked revoked");

    // Expired.
    let (_, short) = api_token::create(&db, new("users:ann", "b", &s, Some(1))).await?;
    assert!(api_token::lookup(&db, &short).await?.is_some());
    db.query("UPDATE api_tokens SET expires_at = time::now() - 1h WHERE name = 'b';").await?.check()?;
    // Answers are reused for a few seconds (and other servers wait that out); this server can forget now.
    assert!(api_token::lookup(&db, &short).await?.is_some(), "the reused answer");
    api_token::forget_lookups();
    assert!(api_token::lookup(&db, &short).await?.is_none());

    // The owner is deactivated: the token is found but its owner is not active.
    let (_, third) = api_token::create(&db, new("users:ann", "c", &s, None)).await?;
    db.query("UPDATE users:ann SET is_active = false;").await?.check()?;
    api_token::forget_lookups();
    let found = api_token::lookup(&db, &third).await?.ok_or("not found")?;
    assert!(!found.user_active);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_token_needs_a_name_and_scopes_and_a_person_has_a_limited_number() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    fresh_core(&db).await?;
    let s = scopes();
    let nothing = Scopes::default();
    for (name, scopes) in [("", &s), ("  ", &s), ("no scopes", &nothing)] {
        let refused = api_token::create(&db, new("users:ann", name, scopes, None)).await;
        assert!(matches!(refused, Err(TokenError::Invalid(_))), "{name:?}: {refused:?}");
    }
    let too_long = "x".repeat(81);
    assert!(api_token::create(&db, new("users:ann", &too_long, &s, None)).await.is_err());

    for n in 0..api_token::MAX_PER_USER {
        api_token::create(&db, new("users:ann", &format!("t{n}"), &s, None)).await?;
    }
    let one_more = api_token::create(&db, new("users:ann", "one more", &s, None)).await;
    assert!(matches!(one_more, Err(TokenError::Invalid(_))), "{one_more:?}");
    // Someone else is not held back by it, and a revoked token makes room.
    api_token::create(&db, new("users:bob", "bobs", &s, None)).await?;
    let first = api_token::list(&db, "users:ann", "org_acme").await?;
    api_token::revoke(&db, &first[0].id, "users:ann", "org_acme").await?;
    api_token::create(&db, new("users:ann", "room again", &s, None)).await?;

    // An owner who is a super user is reported as one (the kernel does not give a token more for it).
    let (_, raw) = api_token::create(&db, new("users:bob", "root-ish", &s, None)).await?;
    assert!(api_token::lookup(&db, &raw).await?.ok_or("not found")?.user_is_super);
    let _ = json!(null);
    Ok(())
}
