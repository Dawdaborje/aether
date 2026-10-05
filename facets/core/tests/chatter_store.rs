//! The chatter tables against a real SurrealDB (see `host_db_audit.rs` for how to run).

use aether_core::chatter::store::{self, Kind, NewMessage};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_chatter_test";

async fn world() -> Result<Option<Surreal<Client>>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.subsec_nanos();
    let org = format!("org_chatter_{suffix}");
    aether_orm::migrate_org(&db, NAMESPACE, &org).await?;
    db.use_ns(NAMESPACE).use_db(&org).await?;
    Ok(Some(db))
}

fn line(kind: Kind, body: &str) -> NewMessage {
    NewMessage {
        model_id: "mdl_a".into(),
        record_key: "r1".into(),
        kind,
        author: "users:ian".into(),
        author_name: None,
        body: Some(body.into()),
        mentions: vec![],
        link: None,
        plugin: None,
    }
}

const ALL: [Kind; 4] = [Kind::Message, Kind::Note, Kind::System, Kind::Change];

#[tokio::test]
#[ignore = "needs AETHER_TEST_DB"]
async fn a_thread_keeps_order_edits_history_and_a_trash() -> TestResult {
    let Some(db) = world().await? else { return Ok(()) };
    let first = store::insert(&db, &line(Kind::Message, "one")).await?;
    store::insert(&db, &line(Kind::Note, "two")).await?;
    store::insert(&db, &NewMessage { record_key: "other".into(), ..line(Kind::Message, "elsewhere") }).await?;

    let all = store::thread(&db, "mdl_a", "r1", &ALL, false, 100).await?;
    assert_eq!(all.iter().map(|m| m.body.as_deref()).collect::<Vec<_>>(), [Some("one"), Some("two")]);
    let public = store::thread(&db, "mdl_a", "r1", &[Kind::Message], false, 100).await?;
    assert_eq!(public.len(), 1, "notes are filtered by kind");

    let edited = store::edit(&db, &first.id, "one, fixed").await?.ok_or("edit")?;
    assert!(edited.edited && edited.edited_at.is_some());
    assert_eq!(edited.body.as_deref(), Some("one, fixed"));

    store::trash(&db, &first.id, "users:ian").await?.ok_or("trash")?;
    assert_eq!(store::thread(&db, "mdl_a", "r1", &ALL, false, 100).await?.len(), 1);
    assert_eq!(store::thread(&db, "mdl_a", "r1", &ALL, true, 100).await?.len(), 2, "trash is kept");
    assert_eq!(store::trashed_count(&db, "mdl_a", "r1").await?, 1);
    assert!(store::edit(&db, &first.id, "no").await?.is_none(), "a trashed message cannot be edited");

    store::restore(&db, &first.id).await?.ok_or("restore")?;
    assert_eq!(store::thread(&db, "mdl_a", "r1", &ALL, false, 100).await?.len(), 2);
    Ok(())
}

#[tokio::test]
#[ignore = "needs AETHER_TEST_DB"]
async fn cleanup_only_removes_trash_older_than_the_retention() -> TestResult {
    let Some(db) = world().await? else { return Ok(()) };
    let old = store::insert(&db, &line(Kind::Message, "old")).await?;
    let recent = store::insert(&db, &line(Kind::Message, "recent")).await?;
    let live = store::insert(&db, &line(Kind::Message, "live")).await?;
    store::trash(&db, &old.id, "users:ian").await?;
    store::trash(&db, &recent.id, "users:ian").await?;
    db.query("UPDATE chatter_messages SET deleted_at = time::now() - 40d WHERE ulid = $id;")
        .bind(("id", old.id.clone()))
        .await?
        .check()?;

    assert_eq!(store::purge(&db, 30).await?, 1);
    assert!(store::get(&db, "mdl_a", "r1", &old.id).await?.is_none());
    assert!(store::get(&db, "mdl_a", "r1", &recent.id).await?.is_some());
    assert!(store::get(&db, "mdl_a", "r1", &live.id).await?.is_some());

    // A deleted record's thread waits too, then goes with the same rule.
    store::trash_thread(&db, "mdl_a", "r1").await?;
    assert!(store::thread(&db, "mdl_a", "r1", &ALL, false, 100).await?.is_empty());
    assert_eq!(store::purge(&db, 30).await?, 0, "just deleted: kept");
    assert_eq!(store::purge(&db, 0).await?, 2, "a zero-day retention clears it");
    Ok(())
}

#[tokio::test]
#[ignore = "needs AETHER_TEST_DB"]
async fn following_is_unique_and_muting_survives_a_second_follow() -> TestResult {
    let Some(db) = world().await? else { return Ok(()) };
    store::follow(&db, "mdl_a", "r1", "users:ian", "creator").await?;
    store::set_muted(&db, "mdl_a", "r1", "users:ian", true).await?;
    store::follow(&db, "mdl_a", "r1", "users:ian", "commenter").await?;
    store::follow(&db, "mdl_a", "r1", "users:bo", "mention").await?;

    let followers = store::followers(&db, "mdl_a", "r1").await?;
    assert_eq!(followers.len(), 2);
    let ian = followers.iter().find(|f| f.actor == "users:ian").ok_or("ian")?;
    assert!(ian.muted && ian.reason == "creator");

    store::unfollow(&db, "mdl_a", "r1", "users:ian").await?;
    assert_eq!(store::followers(&db, "mdl_a", "r1").await?.len(), 1);
    Ok(())
}
