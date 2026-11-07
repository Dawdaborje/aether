//! The audit chain against a real SurrealDB (see `host_db_audit.rs` for how to run): sealing rows,
//! verifying them, and noticing every way of changing them afterwards.

use std::collections::{HashMap, HashSet};

use aether_core::access::audit::{Actor, AuditContext};
use aether_core::access::chain::{AuditKey, Chain, SealOptions, export_anchor, head, purge_before, seal, verify};
use aether_core::kernel::{CallInfo, DbScope, ModelGrant, PluginHostContext, kernel_command};
use aether_core::notifications::NotificationHub;
use serde_json::{Value, json};
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_chain_test";

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

/// Organizations are made one at a time: creating two in one namespace at once can make SurrealDB
/// refuse one with a write conflict, which has nothing to do with what is tested here.
static SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A fresh organization with the real migrations; the handle is on its database.
async fn fresh_org(db: &Surreal<Client>, label: &str) -> Result<String, Box<dyn std::error::Error>> {
    let _one_at_a_time = SETUP.lock().await;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos();
    let name = format!("org_{label}_{unique}");
    aether_orm::migrate_org(db, NAMESPACE, &name).await?;
    db.use_ns(NAMESPACE).use_db(&name).await?;
    Ok(name)
}

fn key() -> AuditKey {
    AuditKey::from_seed([9u8; 32])
}

fn quick() -> SealOptions {
    SealOptions {
        batch: 500,
        checkpoint_rows: 1_000_000,
        checkpoint_secs: 3600 * 24,
    }
}

/// `n` data_access rows, written as the kernel writes them.
async fn rows(db: &Surreal<Client>, n: u32) -> TestResult {
    for i in 0..n {
        db.query(
            "CREATE data_access SET request_id = $r, actor_type = 'user', actor_id = 'users:ann', \
             plugin = 'crm', function_name = 'f', model = 'lead', table_name = 'mdl_x', \
             operation = 'read', record_ids = [], record_count = $i, ip = '10.0.0.1';",
        )
        .bind(("r", format!("req-{i}")))
        .bind(("i", i64::from(i)))
        .await?
        .check()?;
    }
    Ok(())
}

async fn seq_ids(db: &Surreal<Client>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut response = db
        .query("SELECT VALUE <string> id FROM data_access ORDER BY chain_seq;")
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn failure_of(db: &Surreal<Client>, anchor: Option<&aether_core::access::chain::Anchor>) -> Result<(String, Option<u64>), Box<dyn std::error::Error>> {
    let report = verify(db, Chain::DataAccess, anchor).await?;
    assert!(!report.ok, "expected a failure: {report:?}");
    let first = report.failures.first().ok_or("no failure listed")?;
    Ok((first.rule.clone(), first.seq))
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn sealed_rows_form_a_chain_that_verifies() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "seal").await?;
    rows(&db, 5).await?;

    // Nothing is sealed until the sealer runs, and verification says so.
    let before = verify(&db, Chain::DataAccess, None).await?;
    assert!(before.ok);
    assert_eq!((before.rows_checked, before.unsealed), (0, 5));

    let done = seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?;
    assert_eq!(done.sealed, 5);
    let after = verify(&db, Chain::DataAccess, None).await?;
    assert!(after.ok, "{after:?}");
    assert_eq!((after.rows_checked, after.unsealed, after.head_seq), (5, 0, 5));

    // New rows join the end of the same chain.
    rows(&db, 2).await?;
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?.sealed, 2);
    let h = head(&db, Chain::DataAccess).await?;
    assert_eq!(h.seq, 7);
    assert!(verify(&db, Chain::DataAccess, None).await?.ok);
    // Nothing left: a pass does nothing.
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?.sealed, 0);

    // A pass seals one batch at a time.
    rows(&db, 7).await?;
    let small = SealOptions { batch: 3, ..quick() };
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &small).await?.sealed, 3);
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &small).await?.sealed, 3);
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &small).await?.sealed, 1);
    assert!(verify(&db, Chain::DataAccess, None).await?.ok);

    // The other chains are separate.
    assert_eq!(head(&db, Chain::PluginCalls).await?.seq, 0);
    db.query("CREATE plugin_calls SET request_id = 'r', actor_type = 'user', plugin = 'crm', function_name = 'f', status = 200;").await?.check()?;
    db.query("CREATE page_visits SET request_id = 'r', actor_type = 'visitor', path = '/', method = 'GET', status = 200;").await?.check()?;
    assert_eq!(seal(&db, &org, Chain::PluginCalls, &key(), &quick()).await?.sealed, 1);
    assert_eq!(seal(&db, &org, Chain::PageVisits, &key(), &quick()).await?.sealed, 1);
    for chain in Chain::ALL {
        assert!(verify(&db, chain, None).await?.ok, "{chain:?}");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn every_way_of_changing_a_sealed_row_is_noticed() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };

    // Each case starts from a fresh, sealed chain of 6 rows, makes one change, and says what it expects.
    let cases: Vec<(&str, &str, &str, Option<u64>)> = vec![
        ("a value is edited", "UPDATE data_access SET operation = 'create' WHERE chain_seq = 3;", "hash", Some(3)),
        ("an actor is rewritten", "UPDATE data_access SET actor_id = 'users:bob' WHERE chain_seq = 2;", "hash", Some(2)),
        ("the time is rewritten", "UPDATE data_access SET date_created = time::now() - 1d WHERE chain_seq = 4;", "hash", Some(4)),
        ("a stored hash is replaced", "UPDATE data_access SET row_hash = 'ff' WHERE chain_seq = 5;", "hash", Some(5)),
        ("a link is replaced", "UPDATE data_access SET prev_hash = 'ee' WHERE chain_seq = 3;", "link", Some(3)),
        ("a middle row is deleted", "DELETE data_access WHERE chain_seq = 3;", "gap", Some(3)),
        ("the first row is deleted", "DELETE data_access WHERE chain_seq = 1;", "gap", Some(1)),
        ("the last rows are deleted", "DELETE data_access WHERE chain_seq >= 5;", "head", Some(5)),
        ("two rows swap places", "UPDATE data_access SET chain_seq = 99 WHERE chain_seq = 2; UPDATE data_access SET chain_seq = 2 WHERE chain_seq = 3; UPDATE data_access SET chain_seq = 3 WHERE chain_seq = 99;", "link", Some(2)),
        ("a row is forged at the end (it cannot link to the real last row)", "CREATE data_access SET request_id = 'x', actor_type = 'user', plugin = 'p', function_name = 'f', operation = 'read', record_ids = [], record_count = 0, chain_seq = 7, prev_hash = 'aa', row_hash = 'bb';", "link", Some(7)),
    ];
    for (what, attack, rule, seq) in cases {
        let org = fresh_org(&db, "attack").await?;
        rows(&db, 6).await?;
        seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?;
        assert!(verify(&db, Chain::DataAccess, None).await?.ok, "{what}: starts sound");
        db.query(attack).await?.check()?;
        let (found_rule, found_seq) = failure_of(&db, None).await?;
        assert_eq!(found_rule, rule, "{what}");
        if seq.is_some() {
            assert_eq!(found_seq, seq, "{what}");
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn checkpoints_are_signed_and_checked() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "points").await?;
    rows(&db, 4).await?;
    let every_two = SealOptions { checkpoint_rows: 2, ..quick() };
    let done = seal(&db, &org, Chain::DataAccess, &key(), &every_two).await?;
    assert_eq!((done.sealed, done.checkpoint), (4, Some(4)));
    let report = verify(&db, Chain::DataAccess, None).await?;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.checkpoints_checked, 1);

    // A forged signature, a checkpoint of another hash, an unknown key: each is noticed.
    db.query("UPDATE audit_checkpoints SET signature = '00';").await?.check()?;
    assert_eq!(failure_of(&db, None).await?.0, "signature");
    db.query("UPDATE audit_checkpoints SET signature = '00', head_hash = 'ab';").await?.check()?;
    let report = verify(&db, Chain::DataAccess, None).await?;
    assert!(report.failures.iter().any(|f| f.rule == "checkpoint"), "{report:?}");

    let org2 = fresh_org(&db, "points2").await?;
    rows(&db, 3).await?;
    seal(&db, &org2, Chain::DataAccess, &key(), &every_two).await?;
    assert!(verify(&db, Chain::DataAccess, None).await?.ok);
    db.query("DELETE audit_keys;").await?.check()?;
    assert_eq!(failure_of(&db, None).await?.0, "unknown_key");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn an_anchor_kept_elsewhere_catches_a_chain_that_was_cut_back() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "anchor").await?;
    rows(&db, 6).await?;
    let every_two = SealOptions { checkpoint_rows: 2, ..quick() };
    seal(&db, &org, Chain::DataAccess, &key(), &every_two).await?;
    let anchor = export_anchor(&db, Chain::DataAccess).await?.ok_or("no checkpoint to anchor")?;
    assert_eq!((anchor.to_seq, anchor.chain.as_str()), (6, "data_access"));
    assert!(verify(&db, Chain::DataAccess, Some(&anchor)).await?.ok);

    // The attacker removes the last rows and rewrites the head and the checkpoints to match.
    let mut response = db.query("SELECT VALUE row_hash FROM data_access WHERE chain_seq = 4;").await?.check()?;
    let hash: Vec<String> = response.take(0)?;
    db.query("DELETE data_access WHERE chain_seq > 4; DELETE audit_checkpoints; UPDATE audit_chain_head SET seq = 4, hash = $h, checkpoint_seq = 0;")
        .bind(("h", hash[0].clone()))
        .await?
        .check()?;
    assert!(verify(&db, Chain::DataAccess, None).await?.ok, "on its own the shorter chain is consistent");
    let (rule, _) = failure_of(&db, Some(&anchor)).await?;
    assert_eq!(rule, "anchor", "the anchor says the chain was longer");

    // An anchor with a forged signature is refused too.
    let mut forged = anchor.clone();
    forged.signature = "00".into();
    assert_eq!(failure_of(&db, Some(&forged)).await?.0, "anchor");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn purging_old_rows_keeps_the_rest_verifiable() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "purge").await?;
    rows(&db, 3).await?;
    let every_three = SealOptions { checkpoint_rows: 3, ..quick() };
    seal(&db, &org, Chain::DataAccess, &key(), &every_three).await?; // rows 1-3, checkpoint at 3
    rows(&db, 3).await?;
    seal(&db, &org, Chain::DataAccess, &key(), &every_three).await?; // rows 4-6, checkpoint at 6
    rows(&db, 2).await?; // not sealed yet
    db.query("UPDATE data_access SET date_created = time::now() - 40d WHERE chain_seq <= 3;").await?.check()?;

    // Only the prefix that ends at a checkpoint and is old enough goes; unsealed rows never do.
    assert_eq!(purge_before(&db, Chain::DataAccess, 30).await?, 3);
    let h = head(&db, Chain::DataAccess).await?;
    assert_eq!((h.base_seq, h.seq), (3, 6));
    let report = verify(&db, Chain::DataAccess, None).await?;
    assert!(report.ok, "{report:?}");
    assert_eq!((report.rows_checked, report.unsealed), (3, 2));
    assert_eq!(purge_before(&db, Chain::DataAccess, 30).await?, 0, "nothing more is old enough");
    assert_eq!(seq_ids(&db).await?.len(), 5);

    // The chain goes on from where it was, and tampering after a purge is still noticed.
    seal(&db, &org, Chain::DataAccess, &key(), &every_three).await?;
    assert!(verify(&db, Chain::DataAccess, None).await?.ok);
    db.query("UPDATE data_access SET operation = 'delete' WHERE chain_seq = 5;").await?.check()?;
    assert_eq!(failure_of(&db, None).await?, ("hash".to_string(), Some(5)));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn two_sealers_at_once_never_number_a_row_twice() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "race").await?;
    rows(&db, 60).await?;
    let small = SealOptions { batch: 10, ..quick() };
    let (a, b) = (db.clone(), db.clone());
    let (org_a, org_b) = (org.clone(), org.clone());
    let (small_a, small_b) = (small.clone(), small.clone());
    let run = |db: Surreal<Client>, org: String, options: SealOptions| async move {
        let key = key();
        let mut total = 0;
        for _ in 0..12 {
            total += seal(&db, &org, Chain::DataAccess, &key, &options).await.map(|s| s.sealed).unwrap_or(0);
        }
        total
    };
    let (x, y) = tokio::join!(run(a, org_a, small_a), run(b, org_b, small_b));
    // However the work was shared, every row ended up numbered exactly once and the chain is sound.
    let mut done = x + y;
    while seal(&db, &org, Chain::DataAccess, &key(), &small).await?.sealed > 0 {
        done += 1; // anything left over (a pass that lost a race)
    }
    let report = verify(&db, Chain::DataAccess, None).await?;
    assert!(report.ok, "{report:?}");
    assert_eq!((report.rows_checked, report.unsealed), (60, 0));
    assert!(done >= 1);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn rows_the_kernel_writes_seal_and_verify() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "real").await?;
    let models: HashMap<String, ModelGrant> = HashMap::from([(
        "message".to_string(),
        ModelGrant {
            name: "message".into(),
            table: "chat_message".into(),
            can_read: true,
            can_write: true,
            schema: None,
            rules: None,
            direct: false,
        },
    )]);
    let ctx = PluginHostContext::new(
        "chat",
        ["db::query", "db::mutate"].iter().map(|c| c.to_string()).collect::<HashSet<_>>(),
        models,
        std::sync::Arc::new(db.clone()),
        DbScope::new(NAMESPACE, &org),
        NotificationHub::default(),
        CallInfo::new(
            AuditContext {
                actor: Actor::User("users:ann".into()),
                request_id: "req-1".into(),
                ip: Some("203.0.113.0".into()),
                user_agent: None,
            },
            "post_message",
        ),
    );
    let created = kernel_command(&ctx, "db::create", json!({ "model": "message", "data": { "body": "hello" } })).await?;
    let id = created["data"]["id"].clone();
    kernel_command(&ctx, "db::get", json!({ "model": "message", "id": id })).await?;
    kernel_command(&ctx, "db::update", json!({ "model": "message", "id": id, "data": { "body": "hi" } })).await?;
    kernel_command(&ctx, "db::find", json!({ "model": "message" })).await?;

    let done = seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?;
    assert_eq!(done.sealed, 4, "create, read, update and find");
    let report = verify(&db, Chain::DataAccess, None).await?;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.rows_checked, 4);
    // The record ids the kernel stored are part of what is hashed.
    db.query("UPDATE data_access SET record_ids = [] WHERE operation = 'create';").await?.check()?;
    assert_eq!(failure_of(&db, None).await?.0, "hash");
    let _: Value = json!(null);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn the_retention_purge_cuts_a_chain_at_a_checkpoint_and_the_rest_by_date() -> TestResult {
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "retention").await?;
    // The purge finds organizations in the core database's registry.
    let core = format!("core_{org}");
    db.use_ns(NAMESPACE).use_db(&core).await?;
    db.query("CREATE org_databases SET db_name = $org;").bind(("org", org.clone())).await?.check()?;
    db.use_ns(NAMESPACE).use_db(&org).await?;

    rows(&db, 4).await?; // data_access: sealed, with a checkpoint
    seal(&db, &org, Chain::DataAccess, &key(), &SealOptions { checkpoint_rows: 4, ..quick() }).await?;
    rows(&db, 2).await?; // data_access: written after, not sealed
    db.query("CREATE plugin_calls SET request_id = 'a', actor_type = 'user', plugin = 'p', function_name = 'f', status = 200; CREATE plugin_calls SET request_id = 'b', actor_type = 'user', plugin = 'p', function_name = 'f', status = 200;").await?.check()?; // never sealed: no chain yet
    db.query("UPDATE data_access SET date_created = time::now() - 60d WHERE chain_seq != NONE; UPDATE data_access SET date_created = time::now() - 60d WHERE chain_seq = NONE; UPDATE plugin_calls SET date_created = time::now() - 60d WHERE request_id = 'a';").await?.check()?;

    let report = aether_core::access::audit::purge_expired(&db, NAMESPACE, &core, 30).await?;
    assert_eq!(report.data_access, 4, "the sealed prefix, cut at its checkpoint; the unsealed rows stay");
    assert_eq!(report.plugin_calls, 1, "no chain here: removed by date");
    db.use_ns(NAMESPACE).use_db(&org).await?;
    let h = head(&db, Chain::DataAccess).await?;
    assert_eq!((h.base_seq, h.seq), (4, 4));
    let sound = verify(&db, Chain::DataAccess, None).await?;
    assert!(sound.ok, "{sound:?}");
    assert_eq!((sound.rows_checked, sound.unsealed), (0, 2));
    // The unsealed rows are sealed afterwards and the chain carries on from the cut.
    assert_eq!(seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?.sealed, 2);
    assert!(verify(&db, Chain::DataAccess, None).await?.ok);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn the_service_sees_which_chains_wait_and_records_each_verification() -> TestResult {
    use aether_core::access::chain_service::{pending, verify_organization};
    let Some(db) = connect().await? else {
        return Ok(());
    };
    let org = fresh_org(&db, "service").await?;
    assert!(pending(&db).await?.is_empty(), "a new organization has nothing to seal");
    rows(&db, 1).await?;
    db.query("CREATE page_visits SET request_id = 'r', actor_type = 'visitor', path = '/', method = 'GET', status = 200;").await?.check()?;
    assert_eq!(pending(&db).await?, vec![Chain::DataAccess, Chain::PageVisits]);
    seal(&db, &org, Chain::DataAccess, &key(), &quick()).await?;
    assert_eq!(pending(&db).await?, vec![Chain::PageVisits]);

    let reports = verify_organization(&db).await?;
    assert_eq!(reports.len(), 3);
    assert!(reports.iter().all(|r| r.ok));
    db.query("UPDATE data_access SET operation = 'delete';").await?.check()?;
    let reports = verify_organization(&db).await?;
    assert!(!reports.iter().all(|r| r.ok));
    let mut response = db.query("SELECT ok, at FROM audit_verifications ORDER BY at;").await?.check()?;
    let logged: Vec<Value> = response.take(0)?;
    assert_eq!(logged.iter().map(|r| r["ok"].as_bool()).collect::<Vec<_>>(), vec![Some(true), Some(false)]);
    Ok(())
}
